package de.shareui.haru.monet

import android.graphics.Bitmap
import android.graphics.Color
import android.text.TextUtils
import org.telegram.messenger.AndroidUtilities
import org.telegram.messenger.R
import org.telegram.messenger.SvgHelper
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Components.MotionBackgroundDrawable
import java.io.File

object MonetAccentHelper {

    private const val FALLBACK_PATTERN_SLUG = "__monet_default_pattern__"

    private val ACCENT_IDS = intArrayOf(91, 90, 89)

    private var fallbackPatternBitmap: Bitmap? = null
    private var fallbackPatternWidth = 0
    private var fallbackPatternHeight = 0

    @JvmStatic
    fun appendAccentOptions(themeInfo: Theme.ThemeInfo?) {
        if (!isSupported() || themeInfo == null || themeInfo.themeAccentsMap == null || themeInfo.themeAccents == null) {
            return
        }
        for (i in ACCENT_IDS.indices) {
            ensureAccent(themeInfo, ACCENT_IDS[i], i)
        }
    }

    @JvmStatic
    fun refresh(themeInfo: Theme.ThemeInfo?): Boolean {
        if (themeInfo == null) {
            return false
        }
        val needsPatternReload = refreshAccents(themeInfo)
        refreshPreviewColors(themeInfo)
        return needsPatternReload
    }

    @JvmStatic
    fun isMonetAccent(accent: Theme.ThemeAccent?): Boolean {
        if (accent == null) {
            return false
        }
        return accent.id == 91 || accent.id == 90 || accent.id == 89
    }

    @JvmStatic
    fun canEditAccent(accent: Theme.ThemeAccent?): Boolean {
        return accent != null && accent.id >= 100 && !accent.isDefault && !isMonetAccent(accent)
    }

    @JvmStatic
    fun hasRemotePatternWallpaper(accent: Theme.ThemeAccent?): Boolean {
        return accent != null && !TextUtils.isEmpty(accent.patternSlug) && !isFallbackPattern(accent)
    }

    @JvmStatic
    fun isFallbackPattern(accent: Theme.ThemeAccent?): Boolean {
        return accent != null && FALLBACK_PATTERN_SLUG == accent.patternSlug
    }

    @JvmStatic
    fun createFallbackPatternDrawable(
        c1: Int, c2: Int, c3: Int, c4: Int,
        rotation: Int, intensity: Int, preview: Boolean, phase: Int
    ): MotionBackgroundDrawable {
        val drawable = if (c2 == 0) {
            MotionBackgroundDrawable(c1, c1, c1, c1, rotation, preview)
        } else if (c3 != 0 || c4 != 0) {
            MotionBackgroundDrawable(c1, c2, c3, c4, rotation, preview)
        } else {
            MotionBackgroundDrawable(c1, c2, c1, c2, rotation, preview)
        }
        drawable.setPhase(phase)
        drawable.setPatternBitmap(intensity, getFallbackPatternBitmap())
        drawable.setPatternColorFilter(drawable.getPatternColor())
        return drawable
    }

    @JvmStatic
    fun countLeadingMonetAccents(accents: ArrayList<Theme.ThemeAccent>?): Int {
        if (accents == null) {
            return 0
        }
        var count = 0
        while (count < accents.size && isMonetAccent(accents[count])) {
            count++
        }
        return count
    }

    private fun isSupported(): Boolean = MonetUtils.isSupported()

    private fun ensureAccent(themeInfo: Theme.ThemeInfo, id: Int, index: Int) {
        var accent = themeInfo.themeAccentsMap.get(id)
        if (accent == null) {
            accent = Theme.ThemeAccent.create()
            accent.id = id
            accent.parentTheme = themeInfo
            themeInfo.themeAccentsMap.put(id, accent)
            themeInfo.themeAccents.add(accent)
            themeInfo.defaultAccentCount++
        }
        fillAccentValues(themeInfo, accent, index, false)
    }

    private fun refreshAccents(themeInfo: Theme.ThemeInfo): Boolean {
        if (!isSupported() || themeInfo.themeAccentsMap == null) {
            return false
        }
        var needsPatternReload = false
        for (i in ACCENT_IDS.indices) {
            val accent = themeInfo.themeAccentsMap.get(ACCENT_IDS[i])
            if (accent != null) {
                needsPatternReload = needsPatternReload or fillAccentValues(themeInfo, accent, i, true)
            }
        }
        return needsPatternReload
    }

    private fun fillAccentValues(
        themeInfo: Theme.ThemeInfo,
        accent: Theme.ThemeAccent,
        index: Int,
        isRefresh: Boolean
    ): Boolean {
        val oldWallpaperPath = accent.getPathToWallpaper()
        val isDark = themeInfo.isDark()
        val systemAccentColor = MonetUtils.getSystemAccentColor(index, isDark)
        accent.accentColor = systemAccentColor
        accent.accentColor2 = 0
        accent.myMessagesAccentColor = systemAccentColor
        accent.myMessagesGradientAccentColor1 = 0
        accent.myMessagesGradientAccentColor2 = 0
        accent.myMessagesGradientAccentColor3 = 0
        accent.myMessagesAnimated = false
        fillWallpaperValues(accent, systemAccentColor, isDark)
        accent.patternSlug = ""
        accent.patternIntensity = 0f
        accent.patternMotion = false
        if (isRefresh) {
            deleteCachedWallpaper(oldWallpaperPath)
            deleteCachedWallpaper(accent.getPathToWallpaper())
        }
        return isRefresh && hasRemotePatternWallpaper(accent)
    }

    private fun fillWallpaperValues(accent: Theme.ThemeAccent, color: Int, isDark: Boolean) {
        accent.backgroundOverrideColor = (color.toLong() and 0xFFFFFFFFL)
        accent.backgroundGradientOverrideColor1 = 0L
        accent.backgroundGradientOverrideColor2 = 0L
        accent.backgroundGradientOverrideColor3 = 0L
        accent.backgroundRotation = 45
        accent.inBubbleOverrideColor = if (isDark) {
            MonetUtils.getColor("n1_800")
        } else {
            MonetUtils.getColor("n1_50")
        }
    }

    private fun deleteCachedWallpaper(file: File?) {
        if (file != null && file.exists()) {
            file.delete()
        }
    }

    private fun refreshPreviewColors(themeInfo: Theme.ThemeInfo) {
        if (!isSupported() || !themeInfo.isMonet()) {
            return
        }
        when (themeInfo.name) {
            "Monet Light" -> {
                themeInfo.setPreviewBackgroundColor(MonetUtils.getColor("n1_10"))
                themeInfo.setPreviewInColor(MonetUtils.getColor("n1_50"))
                themeInfo.setPreviewOutColor(MonetUtils.getColor("a1_600"))
            }
            "Monet Dark" -> {
                themeInfo.setPreviewBackgroundColor(MonetUtils.getColor("n1_900"))
                themeInfo.setPreviewInColor(MonetUtils.getColor("n1_800"))
                themeInfo.setPreviewOutColor(MonetUtils.getColor("a1_200"))
            }
            "Monet Black" -> {
                themeInfo.setPreviewBackgroundColor(Color.BLACK)
                themeInfo.setPreviewInColor(MonetUtils.getColor("n1_800"))
                themeInfo.setPreviewOutColor(MonetUtils.getColor("a1_200"))
            }
        }
    }

    private fun getFallbackPatternBitmap(): Bitmap? {
        val displaySize = AndroidUtilities.displaySize
        val width = maxOf(1, minOf(displaySize.x, displaySize.y))
        val height = maxOf(1, maxOf(displaySize.x, displaySize.y))
        synchronized(this) {
            val current = fallbackPatternBitmap
            if (current == null || current.isRecycled || fallbackPatternWidth != width || fallbackPatternHeight != height) {
                fallbackPatternBitmap = SvgHelper.getBitmap(R.raw.default_pattern, width, height, Color.BLACK)
                fallbackPatternWidth = width
                fallbackPatternHeight = height
            }
            return fallbackPatternBitmap
        }
    }
}