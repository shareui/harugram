package de.shareui.haru.Sdk

import android.content.Context
import android.net.Uri
import dalvik.system.DexClassLoader
import de.shareui.haru.HaruLocale
import org.telegram.messenger.ApplicationLoader
import org.telegram.messenger.FileLog
import java.io.File
import java.io.FileOutputStream
import java.lang.reflect.Modifier
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import java.util.zip.ZipOutputStream

/**
 * Installs, lists, toggles, deletes and loads Haru SDKs.
 *
 * An SDK ships as a zip (`*.harusdk`) holding `classes.dex`, `haru.yml` and the
 * metadata file. Installing unpacks it into `{filesDir}/sdk/{id}/`, so several
 * SDKs live side by side, each keyed by the `id` from its metadata.
 *
 * Loading goes through [DexClassLoader] with the app class loader as parent, so
 * an SDK compiled against the Telegram stubs resolves `org.telegram.*` against
 * the running app.
 */
object SdkManager {

    const val CLASSES_DEX = "classes.dex"
    const val CLASSES_JAR = "classes.jar"

    private const val TAG = "HaruSdk"
    private const val ROOT_DIR = "sdk"
    private const val ODEX_DIR = "oat"
    private const val KEY_ENABLED_PREFIX = "sdk_enabled_"
    private const val STAGING_DIR = "haru_sdk_staging"
    private const val MAX_ENTRY_BYTES = 64L * 1024 * 1024

    /** Names tried on the entry class when starting / stopping an SDK. */
    private val START_METHODS = arrayOf("main", "init", "onLoad", "start")
    private val STOP_METHODS = arrayOf("stop", "onUnload", "destroy")

    /** Class loaders of SDKs loaded in this process, keyed by SDK id. */
    private val active = LinkedHashMap<String, ClassLoader>()

    enum class Failure { UNREADABLE, NOT_AN_ARCHIVE, NO_MANIFEST, NO_DEX, IO }

    sealed class InstallResult {
        data class Success(val sdk: HaruSdk, val replaced: Boolean) : InstallResult()
        data class Error(val failure: Failure, val detail: String? = null) : InstallResult()
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

    /** Everything currently unpacked, sorted by display name. */
    fun list(): List<HaruSdk> {
        val dirs = rootDir().listFiles() ?: return emptyList()
        return dirs
            .filter { it.isDirectory }
            .mapNotNull { HaruSdk.read(it) }
            .sortedBy { it.name.lowercase() }
    }

    fun find(id: String): HaruSdk? = HaruSdk.read(dirFor(id))

    private fun prefs() = ApplicationLoader.applicationContext
        .getSharedPreferences(HaruLocale.PREFS_NAME, Context.MODE_PRIVATE)

    fun isEnabled(id: String): Boolean = prefs().getBoolean(KEY_ENABLED_PREFIX + id, true)

    private fun writeEnabled(id: String, enabled: Boolean) {
        prefs().edit().putBoolean(KEY_ENABLED_PREFIX + id, enabled).apply()
    }

    /** True while this SDK's dex is loaded in the current process. */
    fun isRunning(id: String): Boolean = active.containsKey(id)

    // endregion

    // region install / uninstall

    fun install(context: Context, uri: Uri): InstallResult {
        val cacheDir = context.cacheDir
        val archive = File(cacheDir, "haru_sdk_install.tmp")
        val staging = File(cacheDir, STAGING_DIR)

        try {
            val input = try {
                context.contentResolver.openInputStream(uri)
            } catch (e: Exception) {
                FileLog.e(e)
                null
            } ?: return InstallResult.Error(Failure.UNREADABLE)

            input.use { source ->
                FileOutputStream(archive).use { target -> source.copyTo(target) }
            }

            staging.deleteRecursively()
            staging.mkdirs()

            try {
                ZipFile(archive).use { zip -> extract(zip, staging) }
            } catch (e: Exception) {
                FileLog.e(e)
                return InstallResult.Error(Failure.NOT_AN_ARCHIVE, e.message)
            }

            val staged = HaruSdk.read(staging)
                ?: return InstallResult.Error(Failure.NO_MANIFEST)
            if (!File(staging, CLASSES_DEX).isFile) {
                return InstallResult.Error(Failure.NO_DEX)
            }

            val target = dirFor(staged.id)
            val replaced = target.exists()
            // A reinstall replaces the whole directory. The copy already loaded in
            // this process keeps its open dex until the app restarts, so give it
            // its teardown hook and drop the loader.
            HaruSdk.read(target)?.let { stop(it) }
            active.remove(staged.id)
            target.deleteRecursively()
            target.parentFile?.mkdirs()
            if (!staging.renameTo(target)) {
                staging.copyRecursively(target, overwrite = true)
                staging.deleteRecursively()
            }

            packDexIntoJar(target)

            val installed = HaruSdk.read(target)
                ?: return InstallResult.Error(Failure.NO_MANIFEST)
            return InstallResult.Success(installed, replaced)
        } catch (e: Exception) {
            FileLog.e(e)
            return InstallResult.Error(Failure.IO, e.message)
        } finally {
            archive.delete()
            staging.deleteRecursively()
        }
    }

    fun uninstall(sdk: HaruSdk): Boolean {
        stop(sdk)
        active.remove(sdk.id)
        prefs().edit().remove(KEY_ENABLED_PREFIX + sdk.id).apply()
        return sdk.dir.deleteRecursively()
    }

    /** Unpacks [zip] into [target], refusing entries that would escape it. */
    private fun extract(zip: ZipFile, target: File) {
        val root = target.canonicalPath + File.separator
        val entries = zip.entries()
        while (entries.hasMoreElements()) {
            val entry: ZipEntry = entries.nextElement()
            val outFile = File(target, entry.name)
            if (!outFile.canonicalPath.startsWith(root)) {
                throw SecurityException("entry escapes the sdk directory: ${entry.name}")
            }
            if (entry.isDirectory) {
                outFile.mkdirs()
                continue
            }
            outFile.parentFile?.mkdirs()
            zip.getInputStream(entry).use { source ->
                FileOutputStream(outFile).use { out ->
                    var written = 0L
                    val buffer = ByteArray(16 * 1024)
                    while (true) {
                        val read = source.read(buffer)
                        if (read <= 0) break
                        written += read
                        if (written > MAX_ENTRY_BYTES) {
                            throw SecurityException("entry too large: ${entry.name}")
                        }
                        out.write(buffer, 0, read)
                    }
                }
            }
        }
    }

    /**
     * Wraps `classes.dex` into `classes.jar`. A raw dex is accepted by
     * [DexClassLoader] only on some releases, a jar is accepted everywhere; and
     * from Android 14 the file the loader opens has to be read-only.
     */
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

    // endregion

    // region loading

    /** Loads every enabled SDK. Called once from `de.shareui.haru.Main` at app start. */
    fun initAll() {
        for (sdk in list()) {
            if (!isEnabled(sdk.id)) {
                continue
            }
            val error = start(sdk)
            if (error != null) {
                FileLog.e("$TAG: failed to start ${sdk.id}", error)
            }
        }
    }

    /**
     * Loads [sdk]'s dex and calls its entry point. Returns the failure, or null
     * on success (including when it was already running).
     */
    fun start(sdk: HaruSdk): Throwable? {
        if (active.containsKey(sdk.id)) {
            return null
        }
        return try {
            val jar = sdk.jarFile
            if (!jar.isFile) {
                // Installed before jar packing existed, or the file went missing.
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
            FileLog.d("$TAG: started ${sdk.id} v${sdk.version}")
            null
        } catch (e: Throwable) {
            active.remove(sdk.id)
            e
        }
    }

    /**
     * Calls the optional teardown hook and forgets the class loader. The dex
     * itself stays mapped until the process restarts — Android has no unload —
     * so the SDK is only fully gone after a restart.
     */
    fun stop(sdk: HaruSdk): Boolean {
        val loader = active.remove(sdk.id) ?: return false
        return try {
            invoke(loader, sdk, STOP_METHODS, required = false)
        } catch (e: Throwable) {
            FileLog.e("$TAG: ${sdk.id} failed to stop", e)
            false
        }
    }

    /**
     * Persists the flag and applies it right away: enabling loads the SDK now,
     * disabling runs its teardown hook. Returns the start failure, if any.
     */
    fun setEnabled(sdk: HaruSdk, enabled: Boolean): Throwable? {
        writeEnabled(sdk.id, enabled)
        return if (enabled) {
            start(sdk)
        } else {
            stop(sdk)
            null
        }
    }

    /**
     * Finds and calls the first matching entry point. Kotlin top-level `fun main()`
     * in `Foo.kt` compiles to `FooKt.main()`, so the declared class is tried both
     * as-is and with the `Kt` suffix; `Context` and no-arg signatures are accepted.
     */
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
        val signatures = arrayOf(arrayOf<Class<*>>(Context::class.java), emptyArray())
        for (clazz in candidates) {
            for (name in names) {
                for (signature in signatures) {
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
                    if (signature.isEmpty()) {
                        method.invoke(target)
                    } else {
                        method.invoke(target, context)
                    }
                    return true
                }
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

    /** Kotlin `object` exposes INSTANCE; otherwise fall back to a no-arg constructor. */
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

    /** Keeps an id from reaching outside `{filesDir}/sdk/`. */
    private fun sanitizeId(id: String): String {
        val cleaned = id.map { if (it.isLetterOrDigit() || it == '.' || it == '_' || it == '-') it else '_' }
            .joinToString("")
            .trim('.')
        return if (cleaned.isEmpty()) "unknown" else cleaned
    }
}
