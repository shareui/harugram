package de.shareui.haru.api

import org.telegram.messenger.AndroidUtilities
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import android.util.Log as AndroidLog

// log buffer behind Logs; sdks call log() directly, no hook needed
// entries are in-memory only and lost when the process dies
object HaruLog {

    const val MAX_ENTRIES = 500

    private const val TAG = "Haru"

    enum class Color {
        DEFAULT, GREEN, YELLOW, RED;

        companion object {
            @JvmStatic
            fun of(name: String?): Color = when (name?.trim()?.lowercase(Locale.US)) {
                "green" -> GREEN
                "yellow" -> YELLOW
                "red" -> RED
                else -> DEFAULT
            }
        }
    }

    // debug lines are hidden unless verbose logging is on
    data class Entry(
        val text: String,
        val color: Color,
        val debug: Boolean,
        val time: Long
    )

    private val entries = ArrayList<Entry>(MAX_ENTRIES)
    private val listeners = CopyOnWriteArrayList<Runnable>()

    @JvmStatic
    @JvmOverloads
    fun log(text: String, color: Color = Color.DEFAULT, debug: Boolean = false) {
        val entry = Entry(text, color, debug, System.currentTimeMillis())
        synchronized(entries) {
            entries.add(entry)
            // drop from the front so the newest lines always survive
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

    @JvmStatic
    fun log(text: String, color: String, debug: Boolean) = log(text, Color.of(color), debug)

    @JvmStatic
    fun entries(): List<Entry> = synchronized(entries) { ArrayList(entries) }

    @JvmStatic
    fun clear() {
        synchronized(entries) { entries.clear() }
        notifyListeners()
    }

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
