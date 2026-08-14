package de.shareui.haru.monet

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Color
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.core.graphics.ColorUtils
import org.telegram.messenger.ApplicationLoader
import org.telegram.messenger.FileLog
import org.telegram.ui.ActionBar.OKLCH
import org.telegram.ui.ActionBar.Theme
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.withSign

/**
 * Access to the system dynamic color palette (Material You / Monet), available since Android 12.
 *
 * The palette lives in framework resources that the system regenerates whenever the wallpaper or
 * the theme overlay changes, so every value is read lazily and dropped again on OVERLAY_CHANGED.
 */
object MonetUtils {

    const val ACCENT_ID_PRIMARY = 91
    const val ACCENT_ID_SECONDARY = 90
    const val ACCENT_ID_TERTIARY = 89

    private const val ACTION_OVERLAY_CHANGED = "android.intent.action.OVERLAY_CHANGED"

    private val ACCENT_PREFIXES = arrayOf("a1_", "a2_", "a3_")
    private val SHADES = intArrayOf(0, 10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000)

    /**
     * Colors below this OKLCH chroma are treated as greys and get tinted with the neutral hue.
     * Kept low on purpose: pale accent tints sit around 0.03 and have to survive untouched.
     */
    private const val NEUTRAL_MAX_CHROMA = 0.02

    /** Same limit Material uses when harmonizing a color towards the key color. */
    private const val MAX_HARMONIZE_DEGREES = 15.0

    private val colorMap = HashMap<String, Int>()
    private var loaded = false
    private var generation = 0

    private var neutralReference: DoubleArray? = null
    private var neutralReferenceGeneration = -1

    private val overlayChangeReceiver = OverlayChangeReceiver()

    @JvmStatic
    fun isSupported(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

    /**
     * Bumped every time the system palette is dropped. Callers that cache derived colors can
     * compare it against the generation they cached with to know when to recompute.
     */
    @JvmStatic
    @Synchronized
    fun getGeneration(): Int = generation

    @JvmStatic
    @Synchronized
    fun invalidate() {
        colorMap.clear()
        loaded = false
        generation++
    }

    /**
     * @param token a palette token: `a1_`/`a2_`/`a3_` for accent ramps, `n1_`/`n2_` for the
     * neutral ones, followed by a shade between 0 and 1000, e.g. `a1_600`.
     */
    @JvmStatic
    @Synchronized
    fun getColor(token: String): Int {
        if (!isSupported()) {
            return 0
        }
        ensureLoaded()
        val color = colorMap[token]
        if (color != null) {
            return color
        }
        FileLog.e("MonetUtils: unknown color token $token")
        return 0
    }

    @JvmStatic
    fun getColor(token: String, fallback: Int): Int {
        val color = getColor(token)
        return if (color == 0) fallback else color
    }

    /**
     * @param index 0 for the primary accent ramp, 1 and 2 for the secondary and tertiary ones.
     */
    @JvmStatic
    fun getSystemAccentColor(index: Int, isDark: Boolean): Int {
        if (!isSupported() || index < 0 || index >= ACCENT_PREFIXES.size) {
            return 0
        }
        return getColor(ACCENT_PREFIXES[index] + if (isDark) "200" else "600")
    }

    /** Maps a Monet accent id back to the palette ramp it was built from. */
    @JvmStatic
    fun getAccentPaletteIndex(accentId: Int): Int = when (accentId) {
        ACCENT_ID_PRIMARY -> 0
        ACCENT_ID_SECONDARY -> 1
        ACCENT_ID_TERTIARY -> 2
        else -> -1
    }

    @JvmStatic
    fun isMonetAccentId(accentId: Int): Boolean = getAccentPaletteIndex(accentId) >= 0

    /** Background tone of a chat under the Monet theme, for the given accent ramp. */
    @JvmStatic
    fun getWallpaperColor(paletteIndex: Int, isDark: Boolean, isBlack: Boolean): Int {
        if (isBlack) {
            return Color.BLACK
        }
        val index = if (paletteIndex in ACCENT_PREFIXES.indices) paletteIndex else 0
        return if (isDark) {
            getColor("n1_900", Color.BLACK)
        } else {
            getColor(ACCENT_PREFIXES[index] + "50", Color.WHITE)
        }
    }

    /** Surface tone used for incoming bubbles and other raised surfaces. */
    @JvmStatic
    fun getSurfaceColor(isDark: Boolean, isBlack: Boolean): Int = when {
        isBlack -> getColor("n1_900", Color.BLACK)
        isDark -> getColor("n1_800", Color.BLACK)
        else -> getColor("n1_50", Color.WHITE)
    }

    /**
     * Rotates [color] towards the hue of [towards] so unrelated palettes (peer colors, gift
     * backdrops) stop clashing with the system accent. Neutral colors are left alone.
     */
    @JvmStatic
    @JvmOverloads
    fun harmonize(color: Int, towards: Int = getColor("a1_600")): Int {
        if (!isSupported() || color == 0 || towards == 0) {
            return color
        }
        val source = OKLCH.rgb2oklch(OKLCH.rgb(color))
        val target = OKLCH.rgb2oklch(OKLCH.rgb(towards))
        if (source[2].isNaN() || target[2].isNaN() || source[1] < NEUTRAL_MAX_CHROMA) {
            return color
        }
        var delta = (target[2] - source[2] + 540.0) % 360.0 - 180.0
        delta = min(abs(delta) * 0.5, MAX_HARMONIZE_DEGREES).withSign(delta)
        source[2] = (source[2] + delta + 360.0) % 360.0
        return toColor(source, Color.alpha(color))
    }

    /**
     * Gives a grey the hue of the system neutral ramp, keeping its lightness so every contrast
     * ratio the theme was designed around survives. Colored values are returned untouched — those
     * are handled by the accent engine in [Theme].
     */
    @JvmStatic
    fun tintNeutral(color: Int): Int {
        if (!isSupported() || Color.alpha(color) == 0) {
            return color
        }
        val reference = neutralReference() ?: return color
        val source = OKLCH.rgb2oklch(OKLCH.rgb(color))
        if (source[1] > NEUTRAL_MAX_CHROMA) {
            return color
        }
        source[1] = reference[0]
        source[2] = reference[1]
        return toColor(source, Color.alpha(color))
    }

    /**
     * Chroma and hue of the neutral ramp as [chroma, hue], cached per palette generation — this is
     * read once per color key of a theme, which is several hundred times per apply.
     */
    @Synchronized
    private fun neutralReference(): DoubleArray? {
        if (neutralReferenceGeneration != generation) {
            neutralReference = null
            val neutral = getColor("n1_500")
            if (neutral != 0) {
                val oklch = OKLCH.rgb2oklch(OKLCH.rgb(neutral))
                if (!oklch[2].isNaN()) {
                    neutralReference = doubleArrayOf(oklch[1], oklch[2])
                }
            }
            neutralReferenceGeneration = generation
        }
        return neutralReference
    }

    /** Pulls a color towards black, used to build the AMOLED variant out of the dark one. */
    @JvmStatic
    fun darken(color: Int, amount: Float): Int {
        if (Color.alpha(color) == 0) {
            return color
        }
        return ColorUtils.setAlphaComponent(
            ColorUtils.blendARGB(color or 0xFF000000.toInt(), Color.BLACK, amount),
            Color.alpha(color)
        )
    }

    @JvmStatic
    fun registerReceiver(context: Context) {
        if (!isSupported()) {
            return
        }
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

    private fun toColor(oklch: DoubleArray, alpha: Int): Int =
        ColorUtils.setAlphaComponent(OKLCH.rgb(OKLCH.oklch2rgb(oklch)), alpha)

    private fun ensureLoaded() {
        if (loaded) {
            return
        }
        val context = ApplicationLoader.applicationContext ?: return
        val resources = context.resources ?: return
        for (palette in 1..3) {
            loadRamp(resources, "a${palette}_", "system_accent${palette}_")
        }
        for (palette in 1..2) {
            loadRamp(resources, "n${palette}_", "system_neutral${palette}_")
        }
        loaded = colorMap.isNotEmpty()
    }

    private fun loadRamp(resources: android.content.res.Resources, token: String, resPrefix: String) {
        for (shade in SHADES) {
            val resId = resources.getIdentifier(resPrefix + shade, "color", "android")
            if (resId != 0) {
                colorMap[token + shade] = resources.getColor(resId, null)
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
            // sent by the system, so the receiver has to stay visible outside the app
            ContextCompat.registerReceiver(
                context.applicationContext,
                this,
                filter,
                ContextCompat.RECEIVER_EXPORTED
            )
            isRegistered = true
        }

        fun unregister(context: Context) {
            if (!isRegistered) {
                return
            }
            context.applicationContext.unregisterReceiver(this)
            isRegistered = false
        }

        override fun onReceive(context: Context, intent: Intent) {
            if (ACTION_OVERLAY_CHANGED != intent.action) {
                return
            }
            Theme.onMonetColorsChanged()
        }
    }
}
