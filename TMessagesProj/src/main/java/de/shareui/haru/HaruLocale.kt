package de.shareui.haru

import android.content.Context
import android.content.res.Configuration
import android.os.Build
import org.telegram.messenger.ApplicationLoader
import org.telegram.messenger.LocaleController
import org.telegram.messenger.R
import org.telegram.tgnet.TLRPC
import java.util.Locale
import kotlin.jvm.JvmStatic

object HaruLocale {

    const val PREFS_NAME = "haru_config"
    const val KEY_LANGUAGE = "haru_language"
    const val KEY_VERBOSE_LOGGING = "haru_verbose_logging"
    const val KEY_SHOW_ID = "haru_show_id"
    const val KEY_ID_SEPARATOR = "haru_id_separator"
    const val DEFAULT_ID_SEPARATOR = "."
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

    /** Fixed brand label — not from LocaleController (cloud packs still say Telegram). */
    const val BRAND_NAME = "Haru"

    @JvmStatic
    fun getBrandName(): String = BRAND_NAME

    /** Show peer ID in profile; enabled by default. */
    @JvmStatic
    fun isShowId(): Boolean =
        prefs().getBoolean(KEY_SHOW_ID, true)

    @JvmStatic
    fun setShowId(enabled: Boolean) {
        prefs().edit().putBoolean(KEY_SHOW_ID, enabled).apply()
    }

    /**
     * Separator used when grouping peer IDs (default `.`).
     * Empty string means no grouping (raw digits).
     */
    @JvmStatic
    fun getIdSeparator(): String {
        val p = prefs()
        if (!p.contains(KEY_ID_SEPARATOR)) {
            return DEFAULT_ID_SEPARATOR
        }
        return p.getString(KEY_ID_SEPARATOR, "") ?: ""
    }

    @JvmStatic
    fun setIdSeparator(separator: String) {
        prefs().edit().putString(KEY_ID_SEPARATOR, separator).apply()
    }

    /** Groups digits with [getIdSeparator]; empty separator → plain id. */
    @JvmStatic
    fun formatIdDotted(id: Long): String {
        val sep = getIdSeparator()
        if (sep.isEmpty()) {
            return id.toString()
        }
        if (sep.length == 1) {
            return LocaleController.formatNumber(id, sep[0])
        }
        // Multi-char separators: group with a placeholder then replace.
        return LocaleController.formatNumber(id, '\u0001').replace("\u0001", sep)
    }

    @JvmStatic
    fun idDigitCount(id: Long): Int =
        id.toString().replace("-", "").length

    @JvmStatic
    fun getUserDcId(user: TLRPC.User?): Int {
        if (user?.photo == null || user.photo is TLRPC.TL_userProfilePhotoEmpty) {
            return 0
        }
        return user.photo.dc_id
    }

    @JvmStatic
    fun getChatDcId(chat: TLRPC.Chat?): Int {
        val photo = chat?.photo ?: return 0
        if (photo is TLRPC.TL_chatPhotoEmpty) {
            return 0
        }
        return photo.dc_id
    }

    // фауст иди нахуй
    /** Primary line: `id: 400.216.230` */
    @JvmStatic
    fun buildIdPrimary(context: Context, id: Long): String =
        getString(context, R.string.HaruIdPrefix) + " " + formatIdDotted(id)

    /**
     * Secondary (gray) line: `(9, DC2)` or `(9)` when [dcId] is 0.
     */
    @JvmStatic
    fun buildIdMeta(context: Context, id: Long, dcId: Int): String {
        val digits = idDigitCount(id)
        return if (dcId > 0) {
            getString(context, R.string.HaruIdMeta, digits, dcId)
        } else {
            getString(context, R.string.HaruIdMetaNoDc, digits)
        }
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

    fun getString(context: Context, resId: Int, vararg formatArgs: Any): String =
        wrap(context).getString(resId, *formatArgs)
}