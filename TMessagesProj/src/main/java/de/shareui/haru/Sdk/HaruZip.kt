package de.shareui.haru.Sdk

import java.io.Closeable
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.RandomAccessFile
import java.util.zip.Inflater
import java.util.zip.InflaterInputStream
import javax.crypto.Cipher
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

/**
 * A small zip reader for archives `java.util.zip` refuses to open.
 *
 * `haru build -p aes256 <password>` encrypts the SDK with WinZip AES: entries
 * are stored under compression method 99 with the real method hidden in the
 * `0x9901` extra field, and no Android release can read that. Everything an SDK
 * archive actually uses is implemented here and nothing else — no zip64, no
 * split archives, no legacy ZipCrypto.
 *
 * Layout of an encrypted entry's data: `salt | 2 password check bytes |
 * ciphertext | 10 byte authentication code`. The key comes from
 * PBKDF2-HMAC-SHA1 over the password, the ciphertext is AES-CTR with a
 * little-endian counter starting at 1, and the authentication code is
 * HMAC-SHA1 of the ciphertext truncated to 10 bytes.
 */
object HaruZip {

    /** The archive is encrypted and the given password does not open it. */
    class WrongPasswordException(message: String) : IOException(message)

    private const val EOCD_SIGNATURE = 0x06054b50
    private const val CENTRAL_SIGNATURE = 0x02014b50
    private const val LOCAL_SIGNATURE = 0x04034b50

    private const val AES_EXTRA_ID = 0x9901
    private const val AES_METHOD = 99
    private const val METHOD_STORED = 0
    private const val METHOD_DEFLATED = 8

    private const val PBKDF2_ITERATIONS = 1000
    private const val PASSWORD_CHECK_BYTES = 2
    private const val AUTH_CODE_BYTES = 10

    /** The comment is at most 64k, so the end record cannot start earlier. */
    private const val EOCD_SEARCH_BYTES = 66_000

    data class Entry(
        val name: String,
        /** The real compression method; for AES entries it comes out of the extra field. */
        val method: Int,
        val compressedSize: Long,
        val encrypted: Boolean,
        /** 1 = AES-128, 2 = AES-192, 3 = AES-256; 0 when the entry is not AES. */
        val aesStrength: Int,
        val localHeaderOffset: Long
    ) {
        val isDirectory: Boolean get() = name.endsWith("/") || name.endsWith("\\")
    }

    /** True when any entry is encrypted. False for a plain or unreadable archive. */
    fun isEncrypted(file: File): Boolean = try {
        open(file)?.use { reader -> reader.entries.any { it.encrypted } } ?: false
    } catch (_: Exception) {
        false
    }

    /** Parses the central directory; null when [file] is not a readable zip. */
    fun open(file: File): Reader? {
        val raf = RandomAccessFile(file, "r")
        return try {
            Reader(raf, readCentralDirectory(raf))
        } catch (e: Exception) {
            try {
                raf.close()
            } catch (_: Exception) {
            }
            if (e is WrongPasswordException) throw e
            null
        }
    }

    class Reader internal constructor(
        private val file: RandomAccessFile,
        val entries: List<Entry>
    ) : Closeable {

        val isEncrypted: Boolean get() = entries.any { it.encrypted }

        /**
         * Opens [entry]'s decrypted, decompressed bytes. Throws
         * [WrongPasswordException] when the password check fails, so a caller can
         * ask again instead of reporting a broken archive.
         */
        fun open(entry: Entry, password: String?): InputStream {
            val dataOffset = dataOffsetOf(entry)
            var raw: InputStream = RegionInputStream(file, dataOffset, entry.compressedSize)

            if (entry.encrypted) {
                if (entry.aesStrength !in 1..3) {
                    throw IOException("${entry.name}: unsupported encryption")
                }
                if (password.isNullOrEmpty()) {
                    throw WrongPasswordException("${entry.name}: password required")
                }
                raw = decrypt(entry, raw, password)
            }

            return when (entry.method) {
                METHOD_STORED -> raw
                METHOD_DEFLATED -> InflaterInputStream(raw, Inflater(true), 16 * 1024)
                else -> throw IOException("${entry.name}: unsupported compression ${entry.method}")
            }
        }

        /** Data starts past the local header, whose name and extra lengths may differ from the central one. */
        private fun dataOffsetOf(entry: Entry): Long {
            val header = ByteArray(30)
            file.seek(entry.localHeaderOffset)
            file.readFully(header)
            if (u32(header, 0) != LOCAL_SIGNATURE.toLong()) {
                throw IOException("${entry.name}: bad local header")
            }
            return entry.localHeaderOffset + 30 + u16(header, 26) + u16(header, 28)
        }

        private fun decrypt(entry: Entry, source: InputStream, password: String): InputStream {
            val keyLength = 8 * (entry.aesStrength + 1)
            val saltLength = keyLength / 2
            val overhead = saltLength + PASSWORD_CHECK_BYTES + AUTH_CODE_BYTES
            if (entry.compressedSize < overhead) {
                throw IOException("${entry.name}: truncated entry")
            }

            val salt = ByteArray(saltLength)
            readFully(source, salt)
            val check = ByteArray(PASSWORD_CHECK_BYTES)
            readFully(source, check)

            // key | mac key | password check, all from one derivation
            val material = pbkdf2(
                password.toByteArray(Charsets.UTF_8),
                salt,
                PBKDF2_ITERATIONS,
                keyLength * 2 + PASSWORD_CHECK_BYTES
            )
            for (i in 0 until PASSWORD_CHECK_BYTES) {
                if (material[keyLength * 2 + i] != check[i]) {
                    throw WrongPasswordException("${entry.name}: wrong password")
                }
            }

            val mac = Mac.getInstance("HmacSHA1")
            mac.init(SecretKeySpec(material, keyLength, keyLength, "HmacSHA1"))

            val cipherLength = entry.compressedSize - overhead
            return AesCtrInputStream(
                source,
                cipherLength,
                material.copyOfRange(0, keyLength),
                mac
            )
        }

        override fun close() {
            try {
                file.close()
            } catch (_: Exception) {
            }
        }
    }

    // region parsing

    private fun readCentralDirectory(file: RandomAccessFile): List<Entry> {
        val length = file.length()
        val tailLength = minOf(length, EOCD_SEARCH_BYTES.toLong()).toInt()
        val tail = ByteArray(tailLength)
        file.seek(length - tailLength)
        file.readFully(tail)

        var eocd = -1
        for (i in tailLength - 22 downTo 0) {
            if (u32(tail, i) == EOCD_SIGNATURE.toLong()) {
                eocd = i
                break
            }
        }
        if (eocd < 0) {
            throw IOException("no end of central directory")
        }

        val count = u16(tail, eocd + 10)
        val size = u32(tail, eocd + 12).toInt()
        val offset = u32(tail, eocd + 16)
        if (count == 0 || size <= 0 || offset < 0 || offset + size > length) {
            throw IOException("bad central directory")
        }

        val central = ByteArray(size)
        file.seek(offset)
        file.readFully(central)

        val entries = ArrayList<Entry>(count)
        var p = 0
        for (i in 0 until count) {
            if (p + 46 > central.size || u32(central, p) != CENTRAL_SIGNATURE.toLong()) {
                throw IOException("bad central header at entry $i")
            }
            val flag = u16(central, p + 8)
            var method = u16(central, p + 10)
            val compressedSize = u32(central, p + 20)
            val nameLength = u16(central, p + 28)
            val extraLength = u16(central, p + 30)
            val commentLength = u16(central, p + 32)
            val localHeaderOffset = u32(central, p + 42)
            val name = String(central, p + 46, nameLength, Charsets.UTF_8)

            var strength = 0
            if (method == AES_METHOD) {
                val aes = findAesExtra(central, p + 46 + nameLength, extraLength)
                    ?: throw IOException("$name: AES entry without its header")
                strength = aes.first
                method = aes.second
            }

            entries.add(
                Entry(
                    name = name,
                    method = method,
                    compressedSize = compressedSize,
                    encrypted = (flag and 1) != 0,
                    aesStrength = strength,
                    localHeaderOffset = localHeaderOffset
                )
            )
            p += 46 + nameLength + extraLength + commentLength
        }
        return entries
    }

    /** Returns the AES strength and the real compression method from the `0x9901` field. */
    private fun findAesExtra(extra: ByteArray, start: Int, length: Int): Pair<Int, Int>? {
        var p = start
        val end = start + length
        while (p + 4 <= end && p + 4 <= extra.size) {
            val id = u16(extra, p)
            val size = u16(extra, p + 2)
            if (p + 4 + size > extra.size) return null
            if (id == AES_EXTRA_ID && size >= 7) {
                val strength = extra[p + 4 + 4].toInt() and 0xff
                val method = u16(extra, p + 4 + 5)
                return strength to method
            }
            p += 4 + size
        }
        return null
    }

    private fun u16(data: ByteArray, offset: Int): Int =
        (data[offset].toInt() and 0xff) or ((data[offset + 1].toInt() and 0xff) shl 8)

    private fun u32(data: ByteArray, offset: Int): Long =
        (u16(data, offset).toLong()) or (u16(data, offset + 2).toLong() shl 16)

    // endregion

    // region crypto

    /** PBKDF2-HMAC-SHA1, spelled out so the password is hashed as raw UTF-8 on every release. */
    private fun pbkdf2(password: ByteArray, salt: ByteArray, iterations: Int, length: Int): ByteArray {
        if (password.isEmpty()) {
            throw WrongPasswordException("empty password")
        }
        val mac = Mac.getInstance("HmacSHA1")
        mac.init(SecretKeySpec(password, "HmacSHA1"))
        val blockLength = mac.macLength
        val out = ByteArray(length)
        var written = 0
        var block = 1
        while (written < length) {
            mac.update(salt)
            mac.update(
                byteArrayOf(
                    (block ushr 24).toByte(),
                    (block ushr 16).toByte(),
                    (block ushr 8).toByte(),
                    block.toByte()
                )
            )
            var u = mac.doFinal()
            val folded = u.copyOf()
            for (i in 1 until iterations) {
                u = mac.doFinal(u)
                for (j in folded.indices) {
                    folded[j] = (folded[j].toInt() xor u[j].toInt()).toByte()
                }
            }
            val take = minOf(blockLength, length - written)
            System.arraycopy(folded, 0, out, written, take)
            written += take
            block++
        }
        return out
    }

    /**
     * AES-CTR over the entry's ciphertext. WinZip counts blocks little-endian
     * from 1, which no `Cipher` mode does, so the counter blocks are encrypted
     * one by one in ECB and xored in.
     */
    private class AesCtrInputStream(
        private val source: InputStream,
        private var remaining: Long,
        key: ByteArray,
        private val mac: Mac
    ) : InputStream() {

        private val cipher = Cipher.getInstance("AES/ECB/NoPadding").apply {
            init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"))
        }
        private val counter = ByteArray(16)
        private val keyStream = ByteArray(16)
        private var keyStreamUsed = 16
        private var verified = false

        init {
            counter[0] = 1
        }

        override fun read(): Int {
            val one = ByteArray(1)
            return if (read(one, 0, 1) == -1) -1 else one[0].toInt() and 0xff
        }

        override fun read(buffer: ByteArray, offset: Int, count: Int): Int {
            if (remaining <= 0L) {
                verify()
                return -1
            }
            val want = minOf(count.toLong(), remaining).toInt()
            val read = source.read(buffer, offset, want)
            if (read <= 0) {
                remaining = 0
                verify()
                return -1
            }
            mac.update(buffer, offset, read)
            for (i in 0 until read) {
                if (keyStreamUsed == 16) {
                    cipher.doFinal(counter, 0, 16, keyStream, 0)
                    increment()
                    keyStreamUsed = 0
                }
                buffer[offset + i] =
                    (buffer[offset + i].toInt() xor keyStream[keyStreamUsed].toInt()).toByte()
                keyStreamUsed++
            }
            remaining -= read
            return read
        }

        /** Compares the trailing authentication code; a mismatch means tampering or truncation. */
        private fun verify() {
            if (verified) return
            verified = true
            val expected = ByteArray(AUTH_CODE_BYTES)
            readFully(source, expected)
            val actual = mac.doFinal()
            var diff = 0
            for (i in 0 until AUTH_CODE_BYTES) {
                diff = diff or (expected[i].toInt() xor actual[i].toInt())
            }
            if (diff != 0) {
                throw IOException("authentication code mismatch")
            }
        }

        private fun increment() {
            for (i in counter.indices) {
                val next = (counter[i].toInt() and 0xff) + 1
                counter[i] = next.toByte()
                if (next <= 0xff) break
            }
        }

        override fun close() {
            // The inflater stops as soon as the deflate stream ends and may never
            // ask for the byte that trips the check, so close is the reliable spot.
            try {
                if (remaining <= 0L) verify()
            } finally {
                source.close()
            }
        }
    }

    // endregion

    /** Reads [length] bytes of [file] starting at [start], seeking on every read. */
    private class RegionInputStream(
        private val file: RandomAccessFile,
        start: Long,
        length: Long
    ) : InputStream() {

        private var position = start
        private var remaining = length

        override fun read(): Int {
            val one = ByteArray(1)
            return if (read(one, 0, 1) == -1) -1 else one[0].toInt() and 0xff
        }

        override fun read(buffer: ByteArray, offset: Int, count: Int): Int {
            if (remaining <= 0L) return -1
            val want = minOf(count.toLong(), remaining).toInt()
            file.seek(position)
            val read = file.read(buffer, offset, want)
            if (read <= 0) return -1
            position += read
            remaining -= read
            return read
        }
    }

    private fun readFully(source: InputStream, buffer: ByteArray) {
        var read = 0
        while (read < buffer.size) {
            val count = source.read(buffer, read, buffer.size - read)
            if (count <= 0) throw IOException("unexpected end of archive")
            read += count
        }
    }
}
