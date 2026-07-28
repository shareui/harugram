package de.shareui.haru.sdk

import android.content.Context
import android.net.Uri
import dalvik.system.DexClassLoader
import de.shareui.haru.HaruLocale
import de.shareui.haru.api.SdkStates
import de.shareui.haru.api.HaruLog
import org.telegram.messenger.ApplicationLoader
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.InputStream
import java.lang.reflect.Modifier
import java.security.MessageDigest
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream

// installs, lists, toggles, deletes and loads haru sdks
// an sdk ships as a zip (*.harusdk) with classes.dex, haru.yml and the metadata file
// installing unpacks it into {filesDir}/sdk/{id}/, each sdk keyed by its metadata id
// loading goes through DexClassLoader with the app class loader as parent
object SdkManager {

    const val CLASSES_DEX = "classes.dex"
    const val CLASSES_JAR = "classes.jar"

    private const val TAG = "HaruSdk"
    private const val ROOT_DIR = "sdk"
    private const val RAW_DIR = "raw"
    private const val ODEX_DIR = "oat"
    private const val KEY_ENABLED_PREFIX = "sdk_enabled_"
    private const val STAGING_DIR = "haru_sdk_staging"
    private const val MAX_ENTRY_BYTES = 64L * 1024 * 1024

    // names tried on the entry class when starting / stopping an sdk
    private val START_METHODS = arrayOf("main", "init", "onLoad", "start")
    private val STOP_METHODS = arrayOf("stop", "onUnload", "destroy")

    // class loaders of sdks loaded in this process, keyed by sdk id
    private val active = LinkedHashMap<String, ClassLoader>()

    enum class Failure { UNREADABLE, NOT_AN_ARCHIVE, NO_MANIFEST, NO_DEX, NO_ENTRY_SIGNATURE, IO }

    sealed class InstallResult {
        data class Success(val sdk: HaruSdk, val replaced: Boolean) : InstallResult()
        data class Error(val failure: Failure, val detail: String? = null) : InstallResult()

        // archive is encrypted (haru build -p ...); wrongPassword tells a first
        // ask apart from a retry after a rejected one
        data class PasswordRequired(val wrongPassword: Boolean) : InstallResult()
    }

    // region storage

    fun rootDir(): File {
        val dir = File(ApplicationLoader.applicationContext.filesDir, ROOT_DIR)
        if (!dir.exists()) {
            dir.mkdirs()
        }
        return dir
    }

    fun dirFor(id: String): File = File(rootDir(), sanitizeId(id))

    private fun rawDir(): File {
        val dir = File(rootDir(), RAW_DIR)
        if (!dir.exists()) {
            dir.mkdirs()
        }
        return dir
    }

    fun rawFileFor(id: String): File = File(rawDir(), "${sanitizeId(id)}.harusdk")

    // index.json is the source of truth; a dir that vanished or lost its
    // manifest behind the app's back is dropped and the index rescanned
    fun list(): List<HaruSdk> {
        val root = rootDir()
        val entries = SdkIndex.load(root)
        val stale = entries.any { entry ->
            val dir = File(root, entry.dirName)
            HaruSdk.read(dir) == null
        }
        if (stale) {
            return rescan()
        }
        return entries.map { it.toSdk(root) }.sortedBy { it.name.lowercase() }
    }

    fun find(id: String): HaruSdk? = list().firstOrNull { it.id == id }

    // rebuilds index.json from whatever haru.yml manifests are actually on disk
    private fun rescan(): List<HaruSdk> {
        val root = rootDir()
        val dirs = root.listFiles() ?: return emptyList()
        val previous = SdkIndex.load(root)
        val rebuilt = dirs
            .filter { it.isDirectory && it.name != RAW_DIR }
            .mapNotNull { dir -> HaruSdk.read(dir) }
        val entries = rebuilt.map { sdk ->
            val existing = previous.firstOrNull { it.id == sdk.id }
            val raw = rawFileFor(sdk.id)
            HaruSdk.toIndexEntry(
                sdk,
                rawPath = existing?.rawPath ?: if (raw.isFile) raw.absolutePath else "",
                rawSha256 = existing?.rawSha256 ?: if (raw.isFile) sha256(raw) else ""
            )
        }
        SdkIndex.save(root, entries)
        return rebuilt.sortedBy { it.name.lowercase() }
    }

    private fun prefs() = ApplicationLoader.applicationContext
        .getSharedPreferences(HaruLocale.PREFS_NAME, Context.MODE_PRIVATE)

    fun isEnabled(id: String): Boolean = prefs().getBoolean(KEY_ENABLED_PREFIX + id, true)

    private fun writeEnabled(id: String, enabled: Boolean) {
        prefs().edit().putBoolean(KEY_ENABLED_PREFIX + id, enabled).apply()
    }

    fun isRunning(id: String): Boolean = active.containsKey(id)

    // endregion

    // region install / uninstall

    // password is only used for archives built with -p; without it such an
    // archive comes back as PasswordRequired so the caller can ask the user
    fun install(context: Context, uri: Uri, password: String? = null): InstallResult {
        val cacheDir = context.cacheDir
        val archive = File(cacheDir, "haru_sdk_install.tmp")
        val staging = File(cacheDir, STAGING_DIR)
        var zipError: Exception? = null

        try {
            val input = try {
                context.contentResolver.openInputStream(uri)
            } catch (e: Exception) {
                HaruLog.App.log(e.toString(), HaruLog.Color.RED)
                null
            } ?: return InstallResult.Error(Failure.UNREADABLE)

            val copied = input.use { source ->
                FileOutputStream(archive).use { target -> source.copyTo(target) }
            }
            if (copied == 0L) {
                // Some providers hand out a Uri they cannot actually stream.
                return InstallResult.Error(Failure.UNREADABLE, "the picked file is empty")
            }

            staging.deleteRecursively()
            staging.mkdirs()

            // java.util.zip cannot open an encrypted archive at all, so those go
            // through the reader that knows WinZip AES.
            if (HaruZip.isEncrypted(archive)) {
                if (password.isNullOrEmpty()) {
                    return InstallResult.PasswordRequired(wrongPassword = false)
                }
                val extracted = try {
                    HaruZip.open(archive)?.use { zip -> extract(zip, staging, password) } ?: 0
                } catch (e: HaruZip.WrongPasswordException) {
                    return InstallResult.PasswordRequired(wrongPassword = true)
                } catch (e: Exception) {
                    HaruLog.App.log("cannot decrypt $archive: $e", HaruLog.Color.RED)
                    return InstallResult.Error(Failure.NOT_AN_ARCHIVE, e.message)
                }
                if (extracted == 0) {
                    return InstallResult.Error(Failure.NOT_AN_ARCHIVE, "$copied bytes: no entries")
                }
                return finish(staging, archive)
            }

            var extracted = try {
                ZipFile(archive).use { zip -> extract(zip, staging) }
            } catch (e: Exception) {
                HaruLog.App.log("$archive ($copied bytes) is not readable as a zip: $e", HaruLog.Color.RED)
                zipError = e
                0
            }

            if (extracted == 0) {
                // ZipFile needs a well-formed central directory; a file that was
                // truncated or padded in transit still reads back entry by entry.
                staging.deleteRecursively()
                staging.mkdirs()
                extracted = try {
                    ZipInputStream(FileInputStream(archive)).use { zip -> extract(zip, staging) }
                } catch (e: Exception) {
                    HaruLog.App.log("streaming read of $archive failed: $e", HaruLog.Color.RED)
                    if (zipError == null) zipError = e
                    0
                }
            }

            if (extracted == 0) {
                val reason = zipError?.message ?: "no entries"
                return InstallResult.Error(Failure.NOT_AN_ARCHIVE, "$copied bytes: $reason")
            }

            return finish(staging, archive)
        } catch (e: Exception) {
            HaruLog.App.log(e.toString(), HaruLog.Color.RED)
            return InstallResult.Error(Failure.IO, e.message)
        } finally {
            archive.delete()
            staging.deleteRecursively()
        }
    }

    // never peek long at 3am
    fun peek(context: Context, uri: Uri, password: String? = null): InstallResult {
        val cacheDir = context.cacheDir
        val archive = File(cacheDir, "haru_sdk_peek.tmp")
        val staging = File(cacheDir, "${STAGING_DIR}_peek")
        try {
            val input = try {
                context.contentResolver.openInputStream(uri)
            } catch (e: Exception) {
                HaruLog.App.log(e.toString(), HaruLog.Color.RED)
                null
            } ?: return InstallResult.Error(Failure.UNREADABLE)

            val copied = input.use { s -> FileOutputStream(archive).use { t -> s.copyTo(t) } }
            if (copied == 0L) return InstallResult.Error(Failure.UNREADABLE, "file is empty")

            staging.deleteRecursively()
            staging.mkdirs()

            if (HaruZip.isEncrypted(archive)) {
                if (password.isNullOrEmpty()) return InstallResult.PasswordRequired(wrongPassword = false)
                val extracted = try {
                    HaruZip.open(archive)?.use { zip -> extract(zip, staging, password) } ?: 0
                } catch (e: HaruZip.WrongPasswordException) {
                    return InstallResult.PasswordRequired(wrongPassword = true)
                } catch (e: Exception) {
                    HaruLog.App.log("peek: cannot decrypt: $e", HaruLog.Color.RED)
                    return InstallResult.Error(Failure.NOT_AN_ARCHIVE, e.message)
                }
                if (extracted == 0) return InstallResult.Error(Failure.NOT_AN_ARCHIVE, "no entries")
            } else {
                var extracted = try {
                    ZipFile(archive).use { zip -> extract(zip, staging) }
                } catch (e: Exception) {
                    0
                }
                if (extracted == 0) {
                    staging.deleteRecursively(); staging.mkdirs()
                    extracted = try {
                        ZipInputStream(FileInputStream(archive)).use { zip -> extract(zip, staging) }
                    } catch (e: Exception) { 0 }
                }
                if (extracted == 0) return InstallResult.Error(Failure.NOT_AN_ARCHIVE, "no entries")
            }

            val sdk = HaruSdk.read(staging)
                ?: return InstallResult.Error(Failure.NO_MANIFEST)
            if (!File(staging, CLASSES_DEX).isFile)
                return InstallResult.Error(Failure.NO_DEX)

            return InstallResult.Success(sdk, replaced = find(sdk.id) != null)
        } catch (e: Exception) {
            HaruLog.App.log("peek failed: $e", HaruLog.Color.RED)
            return InstallResult.Error(Failure.IO, e.message)
        } finally {
            archive.delete()
            staging.deleteRecursively()
        }
    }

    private fun finish(staging: File, archive: File): InstallResult {
        val staged = HaruSdk.read(staging)
            ?: return InstallResult.Error(Failure.NO_MANIFEST)
        if (!File(staging, CLASSES_DEX).isFile) {
            return InstallResult.Error(Failure.NO_DEX)
        }

        // packed here so validation and the final install share the same jar,
        // instead of packing once to check and again after the move
        packDexIntoJar(staging)
        val signatureError = validateEntrySignature(staging, staged)
        if (signatureError != null) {
            return InstallResult.Error(Failure.NO_ENTRY_SIGNATURE, signatureError)
        }

        val target = dirFor(staged.id)
        val replaced = target.exists()
        // a reinstall replaces the whole directory; stop the loaded copy first
        // since its dex stays open until the app restarts
        HaruSdk.read(target)?.let { stop(it) }
        active.remove(staged.id)
        target.deleteRecursively()
        target.parentFile?.mkdirs()
        if (!staging.renameTo(target)) {
            staging.copyRecursively(target, overwrite = true)
            staging.deleteRecursively()
        }

        val installed = HaruSdk.read(target)
            ?: return InstallResult.Error(Failure.NO_MANIFEST)

        val raw = rawFileFor(installed.id)
        archive.copyTo(raw, overwrite = true)
        val hash = sha256(raw)
        SdkIndex.upsert(rootDir(), HaruSdk.toIndexEntry(installed, raw.absolutePath, hash))

        SdkStates.dispatch(
            installed.id,
            if (replaced) SdkStates.Event.UPDATED else SdkStates.Event.INSTALLED
        )
        return InstallResult.Success(installed, replaced)
    }

    // loads the staged jar in isolation to check the entry class exposes a
    // (Context, SdkStates.Self) method before the sdk is ever allowed to run;
    // returns a concrete reason on failure, or null if the signature is found
    private fun validateEntrySignature(staging: File, sdk: HaruSdk): String? {
        val jar = File(staging, CLASSES_JAR)
        val odex = File(staging.parentFile, "haru_sdk_validate_odex")
        odex.deleteRecursively()
        odex.mkdirs()
        try {
            val loader = try {
                DexClassLoader(
                    jar.absolutePath,
                    odex.absolutePath,
                    null,
                    ApplicationLoader.applicationContext.classLoader
                )
            } catch (e: Throwable) {
                return "cannot load classes.jar: ${e.message ?: e.javaClass.simpleName}"
            }

            val candidates = listOfNotNull(
                loadClassOrNull(loader, sdk.entryClass),
                loadClassOrNull(loader, sdk.entryClass + "Kt")
            )
            if (candidates.isEmpty()) {
                return "entry class not found: ${sdk.entryClass}"
            }

            val requiredSignature = arrayOf<Class<*>>(Context::class.java, SdkStates.Self::class.java)
            val hasSignature = candidates.any { clazz ->
                START_METHODS.any { name ->
                    try {
                        clazz.getDeclaredMethod(name, *requiredSignature)
                        true
                    } catch (_: NoSuchMethodException) {
                        false
                    }
                }
            }
            if (!hasSignature) {
                val names = START_METHODS.joinToString("/")
                return "${sdk.entryClass}: no $names(Context, SdkStates.Self)"
            }
            return null
        } finally {
            odex.deleteRecursively()
        }
    }

    fun uninstall(sdk: HaruSdk): Boolean {
        stop(sdk)
        active.remove(sdk.id)
        prefs().edit().remove(KEY_ENABLED_PREFIX + sdk.id).apply()
        val removed = sdk.dir.deleteRecursively()
        if (removed) {
            rawFileFor(sdk.id).delete()
            SdkIndex.remove(rootDir(), sdk.id)
            SdkStates.dispatch(sdk.id, SdkStates.Event.UNINSTALLED)
        }
        return removed
    }

    private fun extract(zip: ZipFile, target: File): Int {
        var files = 0
        val entries = zip.entries()
        while (entries.hasMoreElements()) {
            val entry: ZipEntry = entries.nextElement()
            if (write(entry.name, entry.isDirectory, target) {
                    zip.getInputStream(entry).use { source -> it(source) }
                }
            ) {
                files++
            }
        }
        return files
    }

    // for an archive encrypted with haru build -p
    private fun extract(zip: HaruZip.Reader, target: File, password: String?): Int {
        var files = 0
        for (entry in zip.entries) {
            if (write(entry.name, entry.isDirectory, target) {
                    zip.open(entry, password).use { source -> it(source) }
                }
            ) {
                files++
            }
        }
        return files
    }

    // reads the archive as a stream instead of through its central directory
    private fun extract(zip: ZipInputStream, target: File): Int {
        var files = 0
        while (true) {
            val entry = zip.nextEntry ?: break
            // stream stays open across entries, so it is handed over unclosed
            if (write(entry.name, entry.isDirectory, target) { it(zip) }) {
                files++
            }
            zip.closeEntry()
        }
        return files
    }

    // writes one entry under target, refusing entries that would escape it
    private inline fun write(
        name: String,
        isDirectory: Boolean,
        target: File,
        open: ((InputStream) -> Unit) -> Unit
    ): Boolean {
        val root = target.canonicalPath + File.separator
        val outFile = File(target, name)
        if (!outFile.canonicalPath.startsWith(root)) {
            throw SecurityException("entry escapes the sdk directory: $name")
        }
        if (isDirectory) {
            outFile.mkdirs()
            return false
        }
        outFile.parentFile?.mkdirs()
        open { source ->
            FileOutputStream(outFile).use { out ->
                var written = 0L
                val buffer = ByteArray(16 * 1024)
                while (true) {
                    val read = source.read(buffer)
                    if (read <= 0) break
                    written += read
                    if (written > MAX_ENTRY_BYTES) {
                        throw SecurityException("entry too large: $name")
                    }
                    out.write(buffer, 0, read)
                }
            }
        }
        return true
    }

    // wraps classes.dex into classes.jar: a raw dex only loads on some releases,
    // a jar loads everywhere, and android 14 requires the loaded file to be read-only
    private fun packDexIntoJar(dir: File) {
        val dex = File(dir, CLASSES_DEX)
        val jar = File(dir, CLASSES_JAR)
        if (jar.exists()) {
            jar.setWritable(true)
            jar.delete()
        }
        ZipOutputStream(FileOutputStream(jar)).use { out ->
            out.putNextEntry(ZipEntry(CLASSES_DEX))
            dex.inputStream().use { it.copyTo(out) }
            out.closeEntry()
        }
        jar.setReadOnly()
    }

    private fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(16 * 1024)
            while (true) {
                val read = input.read(buffer)
                if (read <= 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    // endregion

    // region loading

    // called once from Main at app start
    fun initAll() {
        for (sdk in list()) {
            if (!isEnabled(sdk.id)) {
                continue
            }
            val error = start(sdk)
            if (error != null) {
                HaruLog.App.log("failed to start ${sdk.id}: $error", HaruLog.Color.RED)
            }
        }
    }

    fun start(sdk: HaruSdk): Throwable? {
        if (active.containsKey(sdk.id)) {
            return null
        }
        return try {
            val jar = sdk.jarFile
            if (!jar.isFile) {
                // installed before jar packing existed, or the file went missing
                if (!sdk.dexFile.isFile) {
                    throw java.io.FileNotFoundException("${sdk.id}: no $CLASSES_DEX")
                }
                packDexIntoJar(sdk.dir)
            }
            if (jar.canWrite()) {
                jar.setReadOnly()
            }

            val odex = File(sdk.dir, ODEX_DIR)
            odex.mkdirs()
            val loader = DexClassLoader(
                jar.absolutePath,
                odex.absolutePath,
                null,
                ApplicationLoader.applicationContext.classLoader
            )
            invoke(loader, sdk, START_METHODS, required = true)
            active[sdk.id] = loader
            HaruLog.App.log("started ${sdk.id} v${sdk.version}", debug = true)
            SdkStates.dispatch(sdk.id, SdkStates.Event.STARTED)
            null
        } catch (e: Throwable) {
            active.remove(sdk.id)
            e
        }
    }

    // dex stays mapped until the process restarts, android has no unload
    fun stop(sdk: HaruSdk): Boolean {
        val loader = active.remove(sdk.id) ?: return false
        return try {
            invoke(loader, sdk, STOP_METHODS, required = false)
        } catch (e: Throwable) {
            HaruLog.App.log("${sdk.id} failed to stop: $e", HaruLog.Color.RED)
            false
        } finally {
            // loader is gone either way, so the sdk counts as stopped even if teardown threw
            SdkStates.dispatch(sdk.id, SdkStates.Event.STOPPED)
        }
    }

    fun setEnabled(sdk: HaruSdk, enabled: Boolean): Throwable? {
        writeEnabled(sdk.id, enabled)
        SdkStates.dispatch(
            sdk.id,
            if (enabled) SdkStates.Event.ENABLED else SdkStates.Event.DISABLED
        )
        return if (enabled) {
            start(sdk)
        } else {
            stop(sdk)
            null
        }
    }

    // kotlin top-level fun main() in Foo.kt compiles to FooKt.main(), so the
    // declared class is tried both as-is and with the Kt suffix
    private fun invoke(
        loader: ClassLoader,
        sdk: HaruSdk,
        names: Array<String>,
        required: Boolean
    ): Boolean {
        val candidates = listOfNotNull(
            loadClassOrNull(loader, sdk.entryClass),
            loadClassOrNull(loader, sdk.entryClass + "Kt")
        )
        if (candidates.isEmpty()) {
            if (required) throw ClassNotFoundException(sdk.entryClass)
            return false
        }

        val context = ApplicationLoader.applicationContext
        val self = SdkStates.Self(sdk.id)
        // install already rejects anything without this exact signature, so
        // start() only ever needs to look for it
        val signature = arrayOf<Class<*>>(Context::class.java, SdkStates.Self::class.java)
        for (clazz in candidates) {
            for (name in names) {
                val method = try {
                    clazz.getDeclaredMethod(name, *signature)
                } catch (_: NoSuchMethodException) {
                    continue
                }
                method.isAccessible = true
                val target = if (Modifier.isStatic(method.modifiers)) {
                    null
                } else {
                    instanceOf(clazz) ?: continue
                }
                method.invoke(target, context, self)
                return true
            }
        }

        if (required) {
            throw NoSuchMethodException("${sdk.entryClass}: no ${names.joinToString("/")}()")
        }
        return false
    }

    private fun loadClassOrNull(loader: ClassLoader, name: String): Class<*>? = try {
        Class.forName(name, true, loader)
    } catch (_: Throwable) {
        null
    }

    // kotlin object exposes INSTANCE, otherwise fall back to a no-arg constructor
    private fun instanceOf(clazz: Class<*>): Any? {
        try {
            val field = clazz.getDeclaredField("INSTANCE")
            if (Modifier.isStatic(field.modifiers)) {
                field.isAccessible = true
                return field.get(null)
            }
        } catch (_: Throwable) {
        }
        return try {
            val constructor = clazz.getDeclaredConstructor()
            constructor.isAccessible = true
            constructor.newInstance()
        } catch (_: Throwable) {
            null
        }
    }

    // endregion

    // keeps an id from reaching outside {filesDir}/sdk/
    private fun sanitizeId(id: String): String {
        val cleaned = id.map { if (it.isLetterOrDigit() || it == '.' || it == '_' || it == '-') it else '_' }
            .joinToString("")
            .trim('.')
        return if (cleaned.isEmpty()) "unknown" else cleaned
    }
}