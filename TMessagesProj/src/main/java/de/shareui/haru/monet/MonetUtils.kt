package de.shareui.haru.monet

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import com.google.android.material.color.MaterialColors
import org.telegram.messenger.ApplicationLoader
import org.telegram.messenger.FileLog
import org.telegram.ui.ActionBar.Theme

object MonetUtils {

    private const val ACTION_OVERLAY_CHANGED = "android.intent.action.OVERLAY_CHANGED"

    private val ACCENT_PREFIXES = arrayOf("a1_", "a2_", "a3_")

    private val COLOR_MAP = HashMap<String, Int>()

    private val overlayChangeReceiver = OverlayChangeReceiver()

    init {
        if (isSupported()) {
            initSystemColors()
        }
    }

    @JvmStatic
    fun isSupported(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

    @JvmStatic
    fun getColor(colorString: String): Int {
        if (!isSupported()) {
            return 0
        }
        val color = COLOR_MAP[colorString]
        if (color != null) {
            return color
        }
        FileLog.e("MonetUtils: unknown color token $colorString")
        return 0
    }

    @JvmStatic
    fun getSystemAccentColor(index: Int, isDark: Boolean): Int {
        if (!isSupported() || index < 0 || index >= ACCENT_PREFIXES.size) {
            return 0
        }
        val shade = if (isDark) "200" else "600"
        return getColor(ACCENT_PREFIXES[index] + shade)
    }

    @JvmStatic
    fun harmonize(color: Int): Int {
        if (!isSupported()) {
            return color
        }
        val keyColor = getColor("a1_600")
        return MaterialColors.harmonize(color, keyColor)
    }

    @JvmStatic
    fun registerReceiver(context: Context) {
        try {
            overlayChangeReceiver.register(context)
        } catch (e: Exception) {
            FileLog.e(e)
        }
    }

    @JvmStatic
    fun unregisterReceiver(context: Context) {
        try {
            overlayChangeReceiver.unregister(context)
        } catch (e: Exception) {
            FileLog.e(e)
        }
    }

    private fun initSystemColors() {
        val resources = ApplicationLoader.applicationContext.resources
        val packageName = ApplicationLoader.applicationContext.packageName
        for (palette in 1..3) {
            for (shade in intArrayOf(10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900)) {
                val key = "a${palette}_$shade"
                val resName = "system_accent${palette}_$shade"
                val resId = resources.getIdentifier(resName, "color", "android")
                if (resId != 0) {
                    COLOR_MAP[key] = resources.getColor(resId, null)
                }
            }
        }
        for (palette in 1..2) {
            for (shade in intArrayOf(10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900)) {
                val key = "n${palette}_$shade"
                val resName = "system_neutral${palette}_$shade"
                val resId = resources.getIdentifier(resName, "color", "android")
                if (resId != 0) {
                    COLOR_MAP[key] = resources.getColor(resId, null)
                }
            }
        }
    }

    private class OverlayChangeReceiver : BroadcastReceiver() {
        private var isRegistered = false

        fun register(context: Context) {
            if (isRegistered) {
                return
            }
            val filter = IntentFilter(ACTION_OVERLAY_CHANGED)
            filter.addDataScheme("package")
            filter.addDataSchemeSpecificPart(context.packageName, 0)
            context.registerReceiver(this, filter)
            isRegistered = true
        }

        fun unregister(context: Context) {
            if (!isRegistered) {
                return
            }
            context.unregisterReceiver(this)
            isRegistered = false
        }

        override fun onReceive(context: Context, intent: Intent) {
            if (ACTION_OVERLAY_CHANGED != intent.action) {
                return
            }
            Theme.refreshMonetColors()
            if (Theme.isCurrentThemeMonet() || Theme.isCurrentAccentMonet()) {
                Theme.applyTheme(Theme.getActiveTheme(), Theme.isCurrentThemeNight())
            }
        }
    }
}