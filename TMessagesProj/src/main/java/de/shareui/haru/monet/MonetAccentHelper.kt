package de.shareui.haru.monet

import org.telegram.ui.ActionBar.Theme

/**
 * Keeps the three dynamic accents (primary, secondary and tertiary system ramps) in sync with the
 * palette Android generates from the wallpaper. The accents are appended to every theme, so a
 * classic theme can use the system accent too, while the Monet themes select one by default.
 */
object MonetAccentHelper {

    /**
     * A non-zero long whose color part is zero: [Theme.ThemeAccent.fillAccentColors] reads that as
     * "drop the value the theme asset carries" instead of "no override".
     */
    private const val CLEAR_OVERRIDE = 0x100000000L

    private val ACCENT_IDS = intArrayOf(
        MonetUtils.ACCENT_ID_PRIMARY,
        MonetUtils.ACCENT_ID_SECONDARY,
        MonetUtils.ACCENT_ID_TERTIARY
    )

    @JvmStatic
    fun appendAccentOptions(themeInfo: Theme.ThemeInfo?) {
        if (!MonetUtils.isSupported() || themeInfo?.themeAccentsMap == null || themeInfo.themeAccents == null) {
            return
        }
        for (index in ACCENT_IDS.indices) {
            ensureAccent(themeInfo, ACCENT_IDS[index], index)
        }
    }

    /** Re-reads the system palette into the accents and previews of [themeInfo]. */
    @JvmStatic
    fun refresh(themeInfo: Theme.ThemeInfo?) {
        if (!MonetUtils.isSupported() || themeInfo?.themeAccentsMap == null) {
            return
        }
        for (index in ACCENT_IDS.indices) {
            val accent = themeInfo.themeAccentsMap.get(ACCENT_IDS[index]) ?: continue
            fillAccentValues(themeInfo, accent, index)
        }
        refreshPreviewColors(themeInfo)
    }

    @JvmStatic
    fun isMonetAccent(accent: Theme.ThemeAccent?): Boolean =
        accent != null && MonetUtils.isMonetAccentId(accent.id)

    /** Monet accents are derived from the system, so there is nothing for the user to edit. */
    @JvmStatic
    fun canEditAccent(accent: Theme.ThemeAccent?): Boolean =
        accent != null && accent.id >= 100 && !accent.isDefault

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
        fillAccentValues(themeInfo, accent, index)
    }

    private fun fillAccentValues(themeInfo: Theme.ThemeInfo, accent: Theme.ThemeAccent, index: Int) {
        val isDark = themeInfo.isDark()
        val isBlack = themeInfo.isMonetBlack()
        val accentColor = MonetUtils.getSystemAccentColor(index, isDark)
        if (accentColor == 0) {
            return
        }

        accent.accentColor = accentColor
        accent.accentColor2 = 0
        accent.myMessagesAccentColor = accentColor
        accent.myMessagesGradientAccentColor1 = 0
        accent.myMessagesGradientAccentColor2 = 0
        accent.myMessagesGradientAccentColor3 = 0
        accent.myMessagesAnimated = false

        // a flat tonal surface instead of a pattern, which would need a wallpaper download
        accent.backgroundOverrideColor =
            MonetUtils.getWallpaperColor(index, isDark, isBlack).toLong() and 0xFFFFFFFFL
        accent.backgroundGradientOverrideColor1 = CLEAR_OVERRIDE
        accent.backgroundGradientOverrideColor2 = CLEAR_OVERRIDE
        accent.backgroundGradientOverrideColor3 = CLEAR_OVERRIDE
        accent.backgroundRotation = 45
        accent.inBubbleOverrideColor = MonetUtils.getSurfaceColor(isDark, isBlack)
        accent.patternSlug = ""
        accent.patternIntensity = 0f
        accent.patternMotion = false
    }

    private fun refreshPreviewColors(themeInfo: Theme.ThemeInfo) {
        if (!themeInfo.isMonet()) {
            return
        }
        val isDark = themeInfo.isDark()
        val isBlack = themeInfo.isMonetBlack()
        val index = MonetUtils.getAccentPaletteIndex(themeInfo.currentAccentId).coerceAtLeast(0)
        themeInfo.setPreviewBackgroundColor(
            when {
                isBlack -> android.graphics.Color.BLACK
                isDark -> MonetUtils.getColor("n1_900", themeInfo.previewBackgroundColor)
                else -> MonetUtils.getColor("n1_10", themeInfo.previewBackgroundColor)
            }
        )
        themeInfo.setPreviewInColor(MonetUtils.getSurfaceColor(isDark, isBlack))
        themeInfo.setPreviewOutColor(MonetUtils.getSystemAccentColor(index, isDark))
    }
}
