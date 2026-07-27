package de.shareui.haru.sdk

// tiny yaml reader for haru.yml / metadata.yml
// only flat key: value pairs and key: with indented - item lists are supported
// anything more exotic is ignored rather than failing the whole file
object MiniYaml {

    fun parse(text: String): Map<String, Any> {
        val out = LinkedHashMap<String, Any>()
        var pendingKey: String? = null

        for (raw in text.lineSequence()) {
            val line = stripComment(raw)
            if (line.isBlank()) continue
            val trimmed = line.trim()

            if (trimmed.startsWith("-")) {
                val key = pendingKey ?: continue
                val item = unquote(trimmed.substring(1).trim())
                if (item.isEmpty()) continue
                @Suppress("UNCHECKED_CAST")
                val list = out.getOrPut(key) { ArrayList<String>() } as? MutableList<String> ?: continue
                list.add(item)
                continue
            }

            val separator = trimmed.indexOf(':')
            if (separator <= 0) continue
            val key = trimmed.substring(0, separator).trim()
            val value = unquote(trimmed.substring(separator + 1).trim())
            if (value.isEmpty()) {
                // `key:` opens a block; the items that follow belong to it
                pendingKey = key
            } else {
                pendingKey = null
                out[key] = value
            }
        }
        return out
    }

    fun string(map: Map<String, Any>, key: String): String? =
        (map[key] as? String)?.takeIf { it.isNotEmpty() }

    fun list(map: Map<String, Any>, key: String): List<String> {
        return when (val value = map[key]) {
            is List<*> -> value.filterIsInstance<String>()
            is String -> listOf(value)
            else -> emptyList()
        }
    }

    // drops a trailing # comment, but not a # inside a value or a quoted string
    private fun stripComment(line: String): String {
        var quote: Char? = null
        for (i in line.indices) {
            val c = line[i]
            when {
                quote != null -> if (c == quote) quote = null
                c == '"' || c == '\'' -> quote = c
                c == '#' && (i == 0 || line[i - 1].isWhitespace()) -> return line.substring(0, i)
            }
        }
        return line
    }

    private fun unquote(value: String): String {
        if (value.length >= 2) {
            val first = value.first()
            if ((first == '"' || first == '\'') && value.last() == first) {
                return value.substring(1, value.length - 1)
            }
        }
        return value
    }
}
