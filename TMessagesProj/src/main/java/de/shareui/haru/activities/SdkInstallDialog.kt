package de.shareui.haru.activities

import android.content.Context
import android.net.Uri
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import de.shareui.haru.HaruLocale
import de.shareui.haru.api.HaruLog
import de.shareui.haru.sdk.HaruSdk
import de.shareui.haru.sdk.SdkManager
import org.telegram.messenger.AndroidUtilities
import org.telegram.messenger.R
import org.telegram.messenger.Utilities
import org.telegram.ui.ActionBar.AlertDialog
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Components.BulletinFactory
import org.telegram.ui.Components.LayoutHelper

// when clicking on a .harusdk file
fun showSdkInstallDialog(fragment: BaseFragment, uri: Uri) {
    val context = fragment.context ?: fragment.parentActivity ?: return
    val progress = AlertDialog(context, AlertDialog.ALERT_TYPE_SPINNER)
    progress.show()

    Utilities.globalQueue.postRunnable {
        val result = SdkManager.peek(context, uri)
        AndroidUtilities.runOnUIThread {
            try { progress.dismiss() } catch (_: Exception) {}

            when (result) {
                is SdkManager.InstallResult.Success -> {
                    showConfirmDialog(fragment, context, uri, result.sdk, result.replaced)
                }
                is SdkManager.InstallResult.PasswordRequired -> {
                    askPassword(fragment, result.wrongPassword) { password ->
                        Utilities.globalQueue.postRunnable {
                            val peekWithPass = SdkManager.peek(context, uri, password)
                            AndroidUtilities.runOnUIThread {
                                when (peekWithPass) {
                                    is SdkManager.InstallResult.Success ->
                                        showConfirmDialog(fragment, context, uri, peekWithPass.sdk, peekWithPass.replaced, password)
                                    is SdkManager.InstallResult.PasswordRequired ->
                                        showSdkInstallError(fragment, context, R.string.HaruSdkErrorPassword)
                                    is SdkManager.InstallResult.Error ->
                                        showSdkInstallError(fragment, context, mapError(peekWithPass.failure), peekWithPass.detail)
                                }
                            }
                        }
                    }
                }
                is SdkManager.InstallResult.Error -> {
                    showSdkInstallError(fragment, context, mapError(result.failure), result.detail)
                }
            }
        }
    }
}

private fun showConfirmDialog(
    fragment: BaseFragment,
    context: Context,
    uri: Uri,
    sdk: HaruSdk,
    replaced: Boolean,
    password: String? = null
) {
    val rp = fragment.resourceProvider
    val loc = { resId: Int -> HaruLocale.getString(context, resId) }

    val contentView = buildContentView(context, sdk, rp)

    val builder = AlertDialog.Builder(context, rp)
        .setTitle(loc(R.string.HaruSdkInstallTitle))
        .setView(contentView)
        .setPositiveButton(loc(R.string.HaruInstall)) { dialog, _ ->
            dialog.dismiss()
            install(fragment, uri, password) {}
        }
        .setNegativeButton(loc(R.string.Close)) { dialog, _ ->
            dialog.dismiss()
        }

    builder.forceVerticalButtons()
    builder.setPositiveButtonOnTop(true)
    builder.makeCustomMaxHeight()
    builder.setWidth(minOf(AndroidUtilities.dp(320f), AndroidUtilities.displaySize.x * 85 / 100))
    fragment.showDialog(builder.create())
}

private fun buildContentView(context: Context, sdk: HaruSdk, rp: Theme.ResourcesProvider?): View {
    val container = LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(AndroidUtilities.dp(24f), AndroidUtilities.dp(8f), AndroidUtilities.dp(24f), AndroidUtilities.dp(4f))
    }

    fun row(label: String, value: String) {
        if (value.isBlank()) return
        val row = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }

        val labelView = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 14f)
            setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText2, rp))
            text = label
            minWidth = AndroidUtilities.dp(80f)
        }

        val valueView = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 14f)
            setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText, rp))
            text = value
            setPadding(AndroidUtilities.dp(8f), 0, 0, 0)
        }

        row.addView(labelView, LayoutHelper.createLinear(LayoutHelper.WRAP_CONTENT, LayoutHelper.WRAP_CONTENT))
        row.addView(valueView, LayoutHelper.createLinear(0, LayoutHelper.WRAP_CONTENT, 1f))
        container.addView(row, LayoutHelper.createLinear(LayoutHelper.MATCH_PARENT, LayoutHelper.WRAP_CONTENT, 0f, 0f, 0f, 6f))
    }

    val versionText = buildString {
        if (sdk.version.isNotEmpty()) append(sdk.version)
        if (sdk.state.isNotEmpty()) append(" (${sdk.state})")
    }

    row(HaruLocale.getString(context, R.string.HaruSdkFieldPackage), sdk.id)
    row(HaruLocale.getString(context, R.string.HaruSdkFieldVersion), versionText)
    row(HaruLocale.getString(context, R.string.HaruSdkFieldAuthor), sdk.author)
    row(HaruLocale.getString(context, R.string.HaruSdkFieldApp), sdk.appVersion)

    return container
}

private fun showSdkInstallError(
    fragment: BaseFragment,
    context: Context,
    resId: Int,
    detail: String? = null
) {
    val text = HaruLocale.getString(context, resId)
    HaruLog.App.log("sdk install error: $text${if (detail != null) " ($detail)" else ""}", HaruLog.Color.RED)
    BulletinFactory.of(fragment)
        .createErrorBulletin(if (detail != null) "$text\n$detail" else text)
        .show()
}

private fun mapError(failure: SdkManager.Failure): Int = when (failure) {
    SdkManager.Failure.UNREADABLE -> R.string.HaruSdkErrorRead
    SdkManager.Failure.NOT_AN_ARCHIVE -> R.string.HaruSdkErrorArchive
    SdkManager.Failure.NO_MANIFEST -> R.string.HaruSdkErrorManifest
    SdkManager.Failure.NO_DEX -> R.string.HaruSdkErrorDex
    SdkManager.Failure.NO_ENTRY_SIGNATURE -> R.string.SdkSourceError
    SdkManager.Failure.IO -> R.string.HaruSdkErrorInstall
}