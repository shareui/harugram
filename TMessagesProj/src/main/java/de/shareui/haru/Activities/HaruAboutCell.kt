package de.shareui.haru.Activities

import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Outline
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewOutlineProvider
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

// красавчик фауст хуйни навайбкодил

/**
 * Full-width Haru branding header for Preferences.
 *
 * Structure: vertical [LinearLayout]
 *   - app icon (rounded square, no elevation/border)
 *   - brand name
 *   - version
 *
 * Note: do NOT use R.mipmap.ic_launcher — AdaptiveIconDrawable paints as a circle.
 */
class HaruAboutCell(context: Context) : LinearLayout(context) {

    private val nameView: TextView
    private val versionView: TextView
    private val iconView: ImageView

    init {
        orientation = VERTICAL
        gravity = Gravity.CENTER_HORIZONTAL
        setBackgroundColor(0)
        tag = org.telegram.ui.Components.RecyclerListView.TAG_NOT_SECTION
        setPadding(0, AndroidUtilities.dp(28f), 0, AndroidUtilities.dp(24f))

        val cornerRadius = AndroidUtilities.dp(16f).toFloat()

        iconView = ImageView(context).apply {
            // Zoom past adaptive-icon safe padding so art fills the rounded square.
            scaleType = ImageView.ScaleType.CENTER_CROP
            setImageResource(R.drawable.haru_app_icon)
            scaleX = 1.22f
            scaleY = 1.22f
            elevation = 0f
            clipToOutline = true
            outlineProvider = object : ViewOutlineProvider() {
                override fun getOutline(view: View, outline: Outline) {
                    outline.setRoundRect(
                        0,
                        0,
                        view.width.coerceAtLeast(1),
                        view.height.coerceAtLeast(1),
                        cornerRadius
                    )
                }
            }
        }
        addView(
            iconView,
            LayoutHelper.createLinear(72, 72, Gravity.CENTER_HORIZONTAL, 0, 0, 0, 12)
        )

        nameView = TextView(context).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 20f)
            typeface = AndroidUtilities.bold()
            setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText))
            gravity = Gravity.CENTER_HORIZONTAL
            text = HaruLocale.getBrandName()
        }
        addView(
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
        addView(
            versionView,
            LayoutHelper.createLinear(
                LayoutHelper.WRAP_CONTENT,
                LayoutHelper.WRAP_CONTENT,
                Gravity.CENTER_HORIZONTAL,
                0, 4, 0, 0
            )
        )
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec)
        val widthSpec = MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY)
        super.onMeasure(widthSpec, heightMeasureSpec)
        iconView.invalidateOutline()
    }

    fun bind() {
        nameView.text = HaruLocale.getBrandName()
        versionView.text = resolveVersionLabel()
        nameView.setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText))
        versionView.setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText3))
        setBackgroundColor(0)
        tag = org.telegram.ui.Components.RecyclerListView.TAG_NOT_SECTION
        iconView.invalidateOutline()
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
