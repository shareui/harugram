package de.shareui.haru.Activities

import android.content.Context
import android.content.pm.PackageManager
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewOutlineProvider
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import de.shareui.haru.HaruLocale
import org.telegram.messenger.AndroidUtilities
import org.telegram.messenger.ApplicationLoader
import org.telegram.messenger.BuildVars
import org.telegram.messenger.R
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Components.LayoutHelper

/**
 * Full-width centered Haru branding block: app icon, name, version
 * (e.g. 12.9.0 (6966)).
 */
class HaruAboutCell(context: Context) : FrameLayout(context) {

    private val nameView: TextView
    private val versionView: TextView
    private val iconView: ImageView

    init {
        // Root fills the list row; content is centered horizontally.
        val column = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
        }

        iconView = ImageView(context).apply {
            scaleType = ImageView.ScaleType.CENTER_CROP
            setImageResource(R.mipmap.ic_launcher_round)
            clipToOutline = true
            outlineProvider = object : ViewOutlineProvider() {
                override fun getOutline(view: View, outline: android.graphics.Outline) {
                    val size = view.width.coerceAtLeast(1)
                    outline.setRoundRect(0, 0, size, size, size / 2f)
                }
            }
        }
        column.addView(
            iconView,
            LayoutHelper.createLinear(72, 72, Gravity.CENTER_HORIZONTAL, 0, 0, 0, 12)
        )

        nameView = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 20f)
            typeface = AndroidUtilities.bold()
            setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText))
            gravity = Gravity.CENTER_HORIZONTAL
            text = HaruLocale.getString(context, R.string.AppName)
        }
        column.addView(
            nameView,
            LayoutHelper.createLinear(
                LayoutHelper.WRAP_CONTENT,
                LayoutHelper.WRAP_CONTENT,
                Gravity.CENTER_HORIZONTAL
            )
        )

        versionView = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 13f)
            setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText3))
            gravity = Gravity.CENTER_HORIZONTAL
            text = resolveVersionLabel()
        }
        column.addView(
            versionView,
            LayoutHelper.createLinear(
                LayoutHelper.WRAP_CONTENT,
                LayoutHelper.WRAP_CONTENT,
                Gravity.CENTER_HORIZONTAL,
                0, 4, 0, 0
            )
        )

        addView(
            column,
            LayoutHelper.createFrame(
                LayoutHelper.MATCH_PARENT,
                LayoutHelper.WRAP_CONTENT.toFloat(),
                Gravity.CENTER_HORIZONTAL or Gravity.TOP,
                0f, 28f, 0f, 24f
            )
        )
        // Transparent — section decoration paints the white rounded card.
        setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
        layoutParams = LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT)
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec)
        val widthSpec = MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY)
        super.onMeasure(widthSpec, heightMeasureSpec)
    }

    fun bind() {
        nameView.text = HaruLocale.getString(context, R.string.AppName)
        versionView.text = resolveVersionLabel()
        nameView.setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText))
        versionView.setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText3))
        setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
    }

    private fun resolveVersionLabel(): String {
        return try {
            val pm = ApplicationLoader.applicationContext.packageManager
            val pInfo = pm.getPackageInfo(ApplicationLoader.applicationContext.packageName, 0)
            val code = pInfo.versionCode / 10
            val name = pInfo.versionName ?: BuildVars.BUILD_VERSION_STRING
            "$name ($code)"
        } catch (_: PackageManager.NameNotFoundException) {
            BuildVars.BUILD_VERSION_STRING ?: ""
        } catch (_: Exception) {
            BuildVars.BUILD_VERSION_STRING ?: ""
        }
    }
}
