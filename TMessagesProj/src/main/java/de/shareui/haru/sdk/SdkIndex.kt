package de.shareui.haru.sdk

import org.json.JSONArray
import org.json.JSONObject
import java.io.File

// sdk/index.json: one file listing every installed sdk, so list()/find() do
// not need to reparse haru.yml on every call. entries are rebuilt from disk
// whenever a stored dir goes missing or loses its manifest.
data class SdkIndexEntry(
    val id: String,
    val name: String,
    val version: String,
    val state: String,
    val author: String,
    val appVersion: String,
    val source: String,
    val socials: List<String>,
    val entryClass: String,
    val dirName: String,
    val rawPath: String,
    val rawSha256: String
) {
    fun toSdk(root: File): HaruSdk = HaruSdk(
        id = id,
        name = name,
        version = version,
        state = state,
        author = author,
        appVersion = appVersion,
        source = source,
        socials = socials,
        entryClass = entryClass,
        dir = File(root, dirName)
    )
}

object SdkIndex {

    private const val FILE_NAME = "index.json"

    fun file(root: File): File = File(root, FILE_NAME)

    fun load(root: File): List<SdkIndexEntry> {
        val file = file(root)
        if (!file.isFile) return emptyList()
        return try {
            val array = JSONArray(file.readText())
            (0 until array.length()).mapNotNull { i -> parseEntry(array.optJSONObject(i)) }
        } catch (_: Exception) {
            emptyList()
        }
    }

    fun save(root: File, entries: List<SdkIndexEntry>) {
        val array = JSONArray()
        for (entry in entries) {
            array.put(toJson(entry))
        }
        file(root).writeText(array.toString(2))
    }

    fun upsert(root: File, entry: SdkIndexEntry) {
        val entries = load(root).filterNot { it.id == entry.id } + entry
        save(root, entries)
    }

    fun remove(root: File, id: String) {
        val entries = load(root).filterNot { it.id == id }
        save(root, entries)
    }

    private fun parseEntry(json: JSONObject?): SdkIndexEntry? {
        json ?: return null
        val id = json.optString("id").takeIf { it.isNotEmpty() } ?: return null
        val entryClass = json.optString("entryClass").takeIf { it.isNotEmpty() } ?: return null
        val dirName = json.optString("dirName").takeIf { it.isNotEmpty() } ?: return null
        val socials = json.optJSONArray("socials")?.let { arr ->
            (0 until arr.length()).mapNotNull { i -> arr.optString(i).takeIf { it.isNotEmpty() } }
        } ?: emptyList()

        return SdkIndexEntry(
            id = id,
            name = json.optString("name", id),
            version = json.optString("version"),
            state = json.optString("state"),
            author = json.optString("author"),
            appVersion = json.optString("appVersion"),
            source = json.optString("source"),
            socials = socials,
            entryClass = entryClass,
            dirName = dirName,
            rawPath = json.optString("rawPath"),
            rawSha256 = json.optString("rawSha256")
        )
    }

    private fun toJson(entry: SdkIndexEntry): JSONObject = JSONObject().apply {
        put("id", entry.id)
        put("name", entry.name)
        put("version", entry.version)
        put("state", entry.state)
        put("author", entry.author)
        put("appVersion", entry.appVersion)
        put("source", entry.source)
        put("socials", JSONArray(entry.socials))
        put("entryClass", entry.entryClass)
        put("dirName", entry.dirName)
        put("rawPath", entry.rawPath)
        put("rawSha256", entry.rawSha256)
    }
}
