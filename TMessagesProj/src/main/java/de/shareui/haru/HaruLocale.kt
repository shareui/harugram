package de.shareui.haru

import android.content.Context
import android.content.res.Configuration
import android.os.Build
import org.telegram.messenger.ApplicationLoader
import org.telegram.messenger.LocaleController
import org.telegram.messenger.R
import java.util.Locale

object HaruLocale {

    const val PREFS_NAME = "haru_config"
    const val KEY_LANGUAGE = "haru_language"
    const val KEY_VERBOSE_LOGGING = "haru_verbose_logging"

    const val LANG_AUTO = "auto"
    const val LANG_EN = "en"
    const val LANG_RU = "ru"
    const val LANG_DE = "de"

    private val supported = setOf(LANG_EN, LANG_RU, LANG_DE)

    private fun prefs() =
        ApplicationLoader.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun getSavedLanguage(): String =
        prefs().getString(KEY_LANGUAGE, LANG_AUTO) ?: LANG_AUTO

    fun setLanguage(code: String) {
        prefs().edit().putString(KEY_LANGUAGE, code).apply()
    }

    fun isVerboseLogging(): Boolean =
        prefs().getBoolean(KEY_VERBOSE_LOGGING, false)

    fun setVerboseLogging(enabled: Boolean) {
        prefs().edit().putBoolean(KEY_VERBOSE_LOGGING, enabled).apply()
    }

    /** Resolved language used for Haru UI (always en/ru/de). */
    fun getResolvedLanguage(): String {
        val saved = getSavedLanguage()
        if (saved != LANG_AUTO && saved in supported) {
            return saved
        }
        return detectFromTelegram()
    }

    fun detectFromTelegram(): String {
        val info = LocaleController.getInstance().currentLocaleInfo
        val code = (info?.pluralLangCode ?: info?.shortName
            ?: LocaleController.getInstance().currentLocale?.language
            ?: "").lowercase(Locale.US)
        val base = code.substringBefore('-').substringBefore('_')
        return if (base in supported) base else LANG_EN
    }

    fun languageDisplayName(context: Context, code: String): String {
        val resolved = if (code == LANG_AUTO) getResolvedLanguage() else code
        return when (resolved) {
            LANG_RU -> getString(context, R.string.HaruLanguageRussian)
            LANG_DE -> getString(context, R.string.HaruLanguageGerman)
            else -> getString(context, R.string.HaruLanguageEnglish)
        }
    }

    fun wrap(context: Context): Context {
        val lang = getResolvedLanguage()
        val locale = Locale(lang)
        Locale.setDefault(locale)
        val config = Configuration(context.resources.configuration)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            config.setLocale(locale)
            return context.createConfigurationContext(config)
        }
        @Suppress("DEPRECATION")
        config.locale = locale
        @Suppress("DEPRECATION")
        context.resources.updateConfiguration(config, context.resources.displayMetrics)
        return context
    }

    fun getString(context: Context, resId: Int): String =
        wrap(context).getString(resId)
}
