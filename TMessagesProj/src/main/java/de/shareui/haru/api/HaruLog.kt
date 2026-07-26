package de.shareui.haru.api

import org.telegram.messenger.AndroidUtilities
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import android.util.Log as AndroidLog

/**
 * The log buffer behind `de.shareui.haru.Activities.Logs`, and the API SDKs
 * write into.
 *
 * An SDK compiles against the app sources and its dex is loaded with the app
 * class loader as parent, so it just calls [log] — no hook, no reflection:
 *
 * ```
 * HaruLog.log("something happened", HaruLog.Color.GREEN, debug = false)
 * ```
 *
 * Entries live in memory only; the buffer keeps the last [MAX_ENTRIES] lines and
 * is gone when the process dies. Everything is also mirrored into logcat under
 * `Haru`, so a `debug = true` line is still reachable with the fragment closed.
 */
object HaruLog {

    const val MAX_ENTRIES = 500

    private const val TAG = "Haru"

    /** The four colors an SDK may pick for a line. */
    enum class Color {
        DEFAULT, GREEN, YELLOW, RED;

        companion object {
            /** Lenient lookup for callers that pass the color as text; unknown → [DEFAULT]. */
            @JvmStatic
            fun of(name: String?): Color = when (name?.trim()?.lowercase(Locale.US)) {
                "green" -> GREEN
                "yellow" -> YELLOW
                "red" -> RED
                else -> DEFAULT
            }
        }
    }

    /** One buffered line. [debug] lines are hidden unless verbose logging is on. */
    data class Entry(
        val text: String,
        val color: Color,
        val debug: Boolean,
        val time: Long
    )

    private val entries = ArrayList<Entry>(MAX_ENTRIES)
    private val listeners = CopyOnWriteArrayList<Runnable>()

    /**
     * Appends [text] to the log.
     *
     * @param color how the line is painted in the logs fragment
     * @param debug true for a line that only matters while debugging — it is
     *   kept in the buffer but shown only when "Verbose logging" is enabled
     */
    @JvmStatic
    @JvmOverloads
    fun log(text: String, color: Color = Color.DEFAULT, debug: Boolean = false) {
        val entry = Entry(text, color, debug, System.currentTimeMillis())
        synchronized(entries) {
            entries.add(entry)
            // Drop from the front so the newest lines always survive.
            while (entries.size > MAX_ENTRIES) {
                entries.removeAt(0)
            }
        }
        when (color) {
            Color.RED -> AndroidLog.e(TAG, text)
            Color.YELLOW -> AndroidLog.w(TAG, text)
            else -> if (debug) AndroidLog.d(TAG, text) else AndroidLog.i(TAG, text)
        }
        notifyListeners()
    }

    /** Same as [log], with the color given by name (`default`/`green`/`yellow`/`red`). */
    @JvmStatic
    fun log(text: String, color: String, debug: Boolean) = log(text, Color.of(color), debug)

    /** Snapshot of the buffer, oldest first. */
    @JvmStatic
    fun entries(): List<Entry> = synchronized(entries) { ArrayList(entries) }

    @JvmStatic
    fun clear() {
        synchronized(entries) { entries.clear() }
        notifyListeners()
    }

    /** Called on the UI thread whenever the buffer changes. */
    fun addListener(listener: Runnable) {
        listeners.addIfAbsent(listener)
    }

    fun removeListener(listener: Runnable) {
        listeners.remove(listener)
    }

    private fun notifyListeners() {
        if (listeners.isEmpty()) {
            return
        }
        AndroidUtilities.runOnUIThread {
            for (listener in listeners) {
                listener.run()
            }
        }
    }
}
