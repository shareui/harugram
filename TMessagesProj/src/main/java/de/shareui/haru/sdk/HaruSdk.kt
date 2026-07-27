package de.shareui.haru.sdk

import java.io.File

// haru.yml is the manifest shipped in the archive; it names the entry class
// and points at the metadata file, default metadata.yml
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
                // without a declared name, the full id is the honest title
                name = MiniYaml.string(metadata, "name") ?: id,
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

        private fun readMetadata(dir: File, manifest: Map<String, Any>): Map<String, Any>? {
            // the archive is flat, so only the file name of the declared path survives packaging
            val declared = MiniYaml.string(manifest, "metadata") ?: DEFAULT_METADATA
            val file = File(dir, File(declared).name)
            if (!file.isFile) return null
            return try {
                MiniYaml.parse(file.readText())
            } catch (_: Exception) {
                null
            }
        }

        // builds an index entry straight from the extracted dir; rawPath/rawSha256
        // are filled in by the caller once the raw archive has been stored
        fun toIndexEntry(sdk: HaruSdk, rawPath: String, rawSha256: String): SdkIndexEntry = SdkIndexEntry(
            id = sdk.id,
            name = sdk.name,
            version = sdk.version,
            state = sdk.state,
            author = sdk.author,
            appVersion = sdk.appVersion,
            source = sdk.source,
            socials = sdk.socials,
            entryClass = sdk.entryClass,
            dirName = sdk.dir.name,
            rawPath = rawPath,
            rawSha256 = rawSha256
        )
    }
}
