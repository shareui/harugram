package de.shareui.haru.activities

import android.content.Context
import android.text.InputType
import android.util.TypedValue
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import de.shareui.haru.HaruLocale
import org.telegram.messenger.AndroidUtilities
import org.telegram.messenger.R
import org.telegram.ui.ActionBar.ActionBar
import org.telegram.ui.ActionBar.AlertDialog
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Cells.HeaderCell
import org.telegram.ui.Cells.ShadowSectionCell
import org.telegram.ui.Cells.TextCheckCell
import org.telegram.ui.Cells.TextDetailSettingsCell
import org.telegram.ui.Cells.TextSettingsCell
import org.telegram.ui.Components.EditTextBoldCursor
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView

class Debug : BaseFragment() {

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var openLogsRow = -1
    private var verboseLoggingRow = -1
    private var logStorageRow = -1
    private var maxLinesRow = -1
    private var showTimeRow = -1
    private var displayLogsHeaderRow = -1
    private var displayAppLogsRow = -1
    private var displaySdkLogsRow = -1
    private var logsDividerRow = -1
    private var duplicateLogcatRow = -1

    private var rowCount = 0
    private var displayLogsExpanded = false

    private fun updateRows(notify: Boolean) {
        var row = 0
        openLogsRow = row++
        verboseLoggingRow = row++
        logStorageRow = row++
        maxLinesRow = row++
        showTimeRow = row++
        duplicateLogcatRow = row++
        displayLogsHeaderRow = row++
        if (displayLogsExpanded) {
            displayAppLogsRow = row++
            displaySdkLogsRow = row++
        } else {
            displayAppLogsRow = -1
            displaySdkLogsRow = -1
        }
        logsDividerRow = row++
        rowCount = row
        if (notify) {
            listAdapter?.notifyDataSetChanged()
        }
    }

    private fun str(resId: Int): String {
        val ctx = context ?: return ""
        return HaruLocale.getString(ctx, resId)
    }

    override fun createView(context: Context): View {
        actionBar.setBackButtonImage(R.drawable.ic_ab_back)
        actionBar.setAllowOverlayTitle(true)
        actionBar.setTitle(str(R.string.HaruDebugMenu))
        actionBar.setActionBarMenuOnItemClick(object : ActionBar.ActionBarMenuOnItemClick() {
            override fun onItemClick(id: Int) {
                if (id == -1) finishFragment()
            }
        })

        updateRows(false)

        listAdapter = ListAdapter(context)
        fragmentView = FrameLayout(context).apply {
            setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundGray))
        }
        val frameLayout = fragmentView as FrameLayout

        listView = RecyclerListView(context).apply {
            setSections()
            setVerticalScrollBarEnabled(false)
            val animator = androidx.recyclerview.widget.DefaultItemAnimator()
            animator.moveDuration = 350
            animator.addDuration = 350
            animator.removeDuration = 350
            animator.changeDuration = 350
            animator.setInterpolator(org.telegram.ui.Components.CubicBezierInterpolator.EASE_OUT_QUINT)
            animator.setDelayAnimations(false)
            animator.supportsChangeAnimations = false
            itemAnimator = animator
            layoutManager = LinearLayoutManager(context, LinearLayoutManager.VERTICAL, false)
            adapter = listAdapter
            setOnItemClickListener { view, position ->
                when (position) {
                    openLogsRow -> presentFragment(Logs())
                    verboseLoggingRow -> {
                        val enabled = !HaruLocale.isVerboseLogging()
                        HaruLocale.setVerboseLogging(enabled)
                        if (view is TextCheckCell) {
                            view.setChecked(enabled)
                        }
                    }

                    logStorageRow -> {
                        val builder = AlertDialog.Builder(context, resourceProvider)
                        builder.setTitle(str(R.string.HaruLogStorage))
                        val items = arrayOf<CharSequence>(
                            str(R.string.HaruLogStorageFile),
                            str(R.string.HaruLogStorageMemory)
                        )
                        builder.setItems(items) { _, i ->
                            HaruLocale.setLogStorage(if (i == 0) HaruLocale.LOG_STORAGE_FILE else HaruLocale.LOG_STORAGE_MEMORY)
                            listAdapter?.notifyItemChanged(logStorageRow)
                        }
                        showDialog(builder.create())
                    }

                    maxLinesRow -> {
                        val ctx = context ?: return@setOnItemClickListener
                        val resourcesProvider = resourceProvider
                        val builder = AlertDialog.Builder(ctx, resourcesProvider)
                        builder.setTitle(str(R.string.HaruLogMaxLines))

                        val editText = EditTextBoldCursor(ctx).apply {
                            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 16f)
                            setTextColor(
                                Theme.getColor(
                                    Theme.key_dialogTextBlack,
                                    resourcesProvider
                                )
                            )
                            setHintTextColor(
                                Theme.getColor(
                                    Theme.key_groupcreate_hintText,
                                    resourcesProvider
                                )
                            )
                            hint = "500"
                            setFocusable(true)
                            inputType = InputType.TYPE_CLASS_NUMBER
                            imeOptions = android.view.inputmethod.EditorInfo.IME_ACTION_DONE
                            maxLines = 1
                            setSingleLine(true)
                            setPadding(
                                AndroidUtilities.dp(16f),
                                AndroidUtilities.dp(11f),
                                AndroidUtilities.dp(16f),
                                AndroidUtilities.dp(11f)
                            )
                            setCursorWidth(1.5f)
                            setCursorColor(
                                Theme.getColor(
                                    Theme.key_windowBackgroundWhiteBlueText4,
                                    resourcesProvider
                                )
                            )
                            setText(HaruLocale.getLogMaxLines().toString())
                            setSelection(text?.length ?: 0)
                            background = android.graphics.drawable.GradientDrawable().apply {
                                cornerRadius = AndroidUtilities.dp(22f).toFloat()
                                setColor(
                                    Theme.multAlpha(
                                        Theme.getColor(
                                            Theme.key_dialogTextBlack,
                                            resourcesProvider
                                        ),
                                        0.06f
                                    )
                                )
                            }
                        }

                        val container = LinearLayout(ctx).apply {
                            orientation = LinearLayout.VERTICAL
                            addView(
                                editText,
                                LayoutHelper.createLinear(
                                    LayoutHelper.MATCH_PARENT,
                                    LayoutHelper.WRAP_CONTENT,
                                    20f, 9f, 20f, 9f
                                )
                            )
                        }

                        builder.setView(container)
                        builder.setPositiveButton(str(R.string.OK)) { dialog, _ ->
                            val v = editText.text.toString().toIntOrNull()
                            if (v != null && v > 0) {
                                HaruLocale.setLogMaxLines(v)
                                listAdapter?.notifyItemChanged(maxLinesRow)
                                dialog.dismiss()
                            } else {
                                AndroidUtilities.shakeView(editText)
                            }
                        }
                        builder.setNegativeButton(str(R.string.Cancel)) { dialog, _ ->
                            dialog.dismiss()
                        }

                        builder.makeCustomMaxHeight()
                        builder.setWidth(
                            minOf(
                                AndroidUtilities.dp(320f),
                                AndroidUtilities.displaySize.x * 85 / 100
                            )
                        )

                        val dialog = builder.create()
                        dialog.setDismissDialogByButtons(false)
                        dialog.setOnShowListener {
                            editText.requestFocus()
                            AndroidUtilities.showKeyboard(editText)
                        }
                        dialog.setOnDismissListener {
                            AndroidUtilities.hideKeyboard(editText)
                        }
                        editText.setOnEditorActionListener { _, actionId, _ ->
                            if (actionId == android.view.inputmethod.EditorInfo.IME_ACTION_DONE) {
                                val button =
                                    dialog.getButton(android.content.DialogInterface.BUTTON_POSITIVE)
                                button?.performClick()
                                true
                            } else {
                                false
                            }
                        }
                        showDialog(dialog)
                    }

                    showTimeRow -> {
                        val enabled = !HaruLocale.isShowLogTime()
                        HaruLocale.setShowLogTime(enabled)
                        if (view is TextCheckCell) view.setChecked(enabled)
                    }
                    duplicateLogcatRow -> {
                        val enabled = !HaruLocale.isDuplicateToLogcat()
                        HaruLocale.setDuplicateToLogcat(enabled)
                        if (view is TextCheckCell) view.setChecked(enabled)
                    }

                    displayLogsHeaderRow -> {
                        displayLogsExpanded = !displayLogsExpanded
                        updateRows(true)
                    }

                    displayAppLogsRow -> {
                        val enabled = !HaruLocale.isShowAppLogs()
                        HaruLocale.setShowAppLogs(enabled)
                        if (view is TextCheckCell) {
                            view.setChecked(enabled)
                        }
                    }

                    displaySdkLogsRow -> {
                        val enabled = !HaruLocale.isShowSdkLogs()
                        HaruLocale.setShowSdkLogs(enabled)
                        if (view is TextCheckCell) {
                            view.setChecked(enabled)
                        }
                    }
                }
            }
        }
        frameLayout.addView(listView, LayoutHelper.createFrameMatchParent())
        actionBar.setAdaptiveBackground(listView)

        return fragmentView
    }

    override fun onResume() {
        super.onResume()
        actionBar?.setTitle(str(R.string.HaruDebugMenu))
        listAdapter?.notifyDataSetChanged()
    }

    private inner class ListAdapter(private val mContext: Context) :
        RecyclerListView.SelectionAdapter() {

        override fun getItemCount(): Int = rowCount

        override fun isEnabled(holder: RecyclerView.ViewHolder): Boolean {
            val type = holder.itemViewType
            return type == 0 || type == 1 || type == 2 || type == 3
        }

        override fun getItemViewType(position: Int): Int {
            return when (position) {
                openLogsRow -> 0
                verboseLoggingRow, showTimeRow, duplicateLogcatRow, displayAppLogsRow, displaySdkLogsRow -> 1
                logStorageRow, maxLinesRow -> 2
                displayLogsHeaderRow -> 3
                logsDividerRow -> 4
                else -> 0
            }
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                1 -> TextCheckCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                }

                2 -> TextDetailSettingsCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                }

                3 -> ExpandableHeaderCell(mContext)
                4 -> ShadowSectionCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundGray))
                }

                else -> TextSettingsCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                }
            }
            return RecyclerListView.Holder(view)
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when (holder.itemViewType) {
                0 -> {
                    val cell = holder.itemView as TextSettingsCell
                    if (position == openLogsRow) {
                        cell.setText(str(R.string.HaruOpenLogs), true)
                        cell.setIcon(R.drawable.menu_select_quote)
                    }
                }

                1 -> {
                    val cell = holder.itemView as TextCheckCell
                    if (position == verboseLoggingRow) {
                        cell.setTextAndCheck(
                            str(R.string.HaruVerboseLogging),
                            HaruLocale.isVerboseLogging(),
                            true
                        )
                    } else if (position == showTimeRow) {
                        cell.setTextAndCheck(
                            str(R.string.HaruLogShowTime),
                            HaruLocale.isShowLogTime(),
                            true
                        )
                    } else if (position == duplicateLogcatRow) {
                        cell.setTextAndCheck(
                            str(R.string.HaruLogDuplicateLogcat),
                            HaruLocale.isDuplicateToLogcat(),
                            true
                        )
                    } else if (position == displayAppLogsRow) {
                        cell.setTextAndCheck(
                            str(R.string.HaruLogShowApp),
                            HaruLocale.isShowAppLogs(),
                            true
                        )
                    } else if (position == displaySdkLogsRow) {
                        cell.setTextAndCheck(
                            str(R.string.HaruLogShowSdk),
                            HaruLocale.isShowSdkLogs(),
                            false
                        )
                    }
                }

                2 -> {
                    val cell = holder.itemView as TextDetailSettingsCell
                    if (position == logStorageRow) {
                        val isFile = HaruLocale.getLogStorage() == HaruLocale.LOG_STORAGE_FILE
                        cell.setTextAndValue(
                            str(R.string.HaruLogStorage),
                            if (isFile) str(R.string.HaruLogStorageFile) else str(R.string.HaruLogStorageMemory),
                            true
                        )
                    } else if (position == maxLinesRow) {
                        cell.setTextAndValue(
                            str(R.string.HaruLogMaxLines),
                            HaruLocale.getLogMaxLines().toString(),
                            true
                        )
                    }
                }

                3 -> {
                    val cell = holder.itemView as ExpandableHeaderCell
                    if (position == displayLogsHeaderRow) {
                        cell.textView.text = str(R.string.HaruLogDisplay)
                        cell.update(displayLogsExpanded)
                    }
                }

                4 -> {
                    val cell = holder.itemView as ShadowSectionCell
                    // divider
                }
            }
        }

        private inner class ExpandableHeaderCell(context: Context) : FrameLayout(context) {
            val textView = android.widget.TextView(context)
            private val arrowView = android.widget.ImageView(context)

            init {
                setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                textView.apply {
                    textSize = 16f
                    setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText))
                    gravity =
                        android.view.Gravity.CENTER_VERTICAL or if (org.telegram.messenger.LocaleController.isRTL) android.view.Gravity.RIGHT else android.view.Gravity.LEFT
                }
                addView(
                    textView,
                    LayoutHelper.createFrame(
                        LayoutHelper.MATCH_PARENT,
                        LayoutHelper.MATCH_PARENT.toFloat(),
                        android.view.Gravity.TOP or android.view.Gravity.LEFT,
                        21f,
                        0f,
                        40f,
                        0f
                    )
                )

                arrowView.apply {
                    setImageResource(R.drawable.arrow_more)
                    colorFilter = android.graphics.PorterDuffColorFilter(
                        Theme.getColor(Theme.key_windowBackgroundWhiteGrayIcon),
                        android.graphics.PorterDuff.Mode.MULTIPLY
                    )
                    scaleType = android.widget.ImageView.ScaleType.CENTER
                }
                addView(
                    arrowView,
                    LayoutHelper.createFrame(
                        24,
                        24f,
                        android.view.Gravity.CENTER_VERTICAL or if (org.telegram.messenger.LocaleController.isRTL) android.view.Gravity.LEFT else android.view.Gravity.RIGHT,
                        16f,
                        0f,
                        16f,
                        0f
                    )
                )
            }

            override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
                super.onMeasure(
                    View.MeasureSpec.makeMeasureSpec(
                        View.MeasureSpec.getSize(widthMeasureSpec),
                        View.MeasureSpec.EXACTLY
                    ),
                    View.MeasureSpec.makeMeasureSpec(
                        AndroidUtilities.dp(50f),
                        View.MeasureSpec.EXACTLY
                    )
                )
            }

            fun update(expanded: Boolean) {
                arrowView.clearAnimation()
                arrowView.animate().rotation(if (expanded) 180f else 0f)
                    .setInterpolator(org.telegram.ui.Components.CubicBezierInterpolator.EASE_OUT_QUINT)
                    .setDuration(240).start()
            }
        }
    }
}