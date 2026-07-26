package de.shareui.haru.Sdk

import java.io.File

/**
 * An SDK unpacked into `{filesDir}/sdk/{id}/`.
 *
 * `haru.yml` is the manifest that ships inside the archive; it names the entry
 * [entryClass] and points at the metadata file (`metadata: metadata.yml` by
 * default). When that file is missing the metadata keys are read straight out
 * of `haru.yml` instead.
 */
data class HaruSdk(
    val id: String,
    val name: String,
    val version: String,
    val state: String,
    val author: String,
    val appVersion: String,
    val source: String,
    val socials: List<String>,
    val entryClass: String,
    val dir: File
) {
    val dexFile: File get() = File(dir, SdkManager.CLASSES_DEX)
    val jarFile: File get() = File(dir, SdkManager.CLASSES_JAR)

    companion object {
        const val HARU_YML = "haru.yml"
        const val DEFAULT_METADATA = "metadata.yml"

        /** Reads the manifest pair out of [dir]; null when there is no usable `haru.yml`. */
        fun read(dir: File): HaruSdk? {
            val manifestFile = File(dir, HARU_YML)
            if (!manifestFile.isFile) return null

            val manifest = try {
                MiniYaml.parse(manifestFile.readText())
            } catch (_: Exception) {
                return null
            }

            val metadata = readMetadata(dir, manifest) ?: manifest
            val id = MiniYaml.string(metadata, "id") ?: dir.name
            val entryClass = MiniYaml.string(manifest, "class")
                ?: MiniYaml.string(metadata, "class")
                ?: return null

            return HaruSdk(
                id = id,
                name = MiniYaml.string(metadata, "name") ?: id.substringAfterLast('.'),
                version = MiniYaml.string(metadata, "version") ?: "",
                state = MiniYaml.string(metadata, "state") ?: "",
                author = MiniYaml.string(metadata, "author") ?: "",
                appVersion = MiniYaml.string(metadata, "app_version") ?: "",
                source = MiniYaml.string(metadata, "source") ?: "",
                socials = MiniYaml.list(metadata, "socials"),
                entryClass = entryClass,
                dir = dir
            )
        }

        /** Resolves `metadata:` from [manifest] relative to [dir]; null when it is absent. */
        private fun readMetadata(dir: File, manifest: Map<String, Any>): Map<String, Any>? {
            // The archive is flat, so only the file name of the declared path survives packaging.
            val declared = MiniYaml.string(manifest, "metadata") ?: DEFAULT_METADATA
            val file = File(dir, File(declared).name)
            if (!file.isFile) return null
            return try {
                MiniYaml.parse(file.readText())
            } catch (_: Exception) {
                null
            }
        }
    }
}
