package de.shareui.haru.activities

import android.content.Context
import android.graphics.Typeface
import android.text.SpannableStringBuilder
import android.text.style.ForegroundColorSpan
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import de.shareui.haru.HaruLocale
import de.shareui.haru.api.HaruLog
import org.telegram.messenger.AndroidUtilities
import org.telegram.messenger.LocaleController
import org.telegram.messenger.R
import org.telegram.ui.ActionBar.ActionBar
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Cells.TextInfoPrivacyCell
import org.telegram.ui.Components.BulletinFactory
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView
import java.text.SimpleDateFormat
import java.util.Locale

// shows what HaruLog collected this session, opened from Debug
class Logs : BaseFragment() {

    companion object {
        private const val MENU_COPY_ALL = 1
        private const val MENU_CLEAR = 2

        // fixed pattern: a log timestamp is read, not localized
        private val timeFormat = SimpleDateFormat("HH:mm:ss.SSS", Locale.US)
    }

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var entries: List<HaruLog.Entry> = emptyList()

    private var emptyRow = -1
    private var rowCount = 0

    private val onLogChanged = Runnable { refresh(scrollToEnd = true) }

    override fun onFragmentCreate(): Boolean {
        super.onFragmentCreate()
        reload()
        HaruLog.addListener(onLogChanged)
        return true
    }

    override fun onFragmentDestroy() {
        HaruLog.removeListener(onLogChanged)
        super.onFragmentDestroy()
    }

    private fun str(resId: Int): String {
        val ctx = context ?: return ""
        return HaruLocale.getString(ctx, resId)
    }

    private fun reload() {
        val verbose = HaruLocale.isVerboseLogging()
        entries = HaruLog.entries().filter { verbose || !it.debug }
        rowCount = entries.size
        emptyRow = if (entries.isEmpty()) rowCount++ else -1
    }

    override fun createView(context: Context): View {
        actionBar.setBackButtonImage(R.drawable.ic_ab_back)
        actionBar.setAllowOverlayTitle(true)
        actionBar.setTitle(str(R.string.HaruLogs))
        actionBar.setActionBarMenuOnItemClick(object : ActionBar.ActionBarMenuOnItemClick() {
            override fun onItemClick(id: Int) {
                when (id) {
                    -1 -> finishFragment()
                    MENU_COPY_ALL -> copyAll()
                    MENU_CLEAR -> {
                        HaruLog.clear()
                        refresh(scrollToEnd = false)
                    }
                }
            }
        })
        // Added first so it sits left of the clear button.
        actionBar.createMenu().addItem(MENU_COPY_ALL, R.drawable.msg_copy)
        actionBar.createMenu().addItem(MENU_CLEAR, R.drawable.msg_delete)

        listAdapter = ListAdapter(context)
        fragmentView = FrameLayout(context).apply {
            setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundGray))
        }
        val frameLayout = fragmentView as FrameLayout

        listView = RecyclerListView(context).apply {
            setVerticalScrollBarEnabled(false)
            layoutManager = LinearLayoutManager(context, LinearLayoutManager.VERTICAL, false)
            adapter = listAdapter
            setOnItemClickListener { _, position ->
                val entry = entries.getOrNull(position) ?: return@setOnItemClickListener
                AndroidUtilities.addToClipboard(entry.text)
                BulletinFactory.of(this@Logs).createCopyBulletin(str(R.string.HaruLogCopied)).show()
            }
        }
        frameLayout.addView(listView, LayoutHelper.createFrameMatchParent())
        actionBar.setAdaptiveBackground(listView)

        scrollToEnd()

        return fragmentView
    }

    override fun onResume() {
        super.onResume()
        actionBar?.setTitle(str(R.string.HaruLogs))
        // The verbose switch may have been flipped while this fragment sat in
        // the back stack, which changes what is listed.
        refresh(scrollToEnd = true)
    }

    private fun refresh(scrollToEnd: Boolean) {
        reload()
        listAdapter?.notifyDataSetChanged()
        if (scrollToEnd) {
            scrollToEnd()
        }
    }

    private fun scrollToEnd() {
        if (rowCount > 0) {
            listView?.scrollToPosition(rowCount - 1)
        }
    }

    private fun copyAll() {
        if (entries.isEmpty()) {
            return
        }
        val dump = entries.joinToString("\n") { "${timeFormat.format(it.time)}  ${it.text}" }
        AndroidUtilities.addToClipboard(dump)
        BulletinFactory.of(this).createCopyBulletin(str(R.string.HaruLogsCopied)).show()
    }

    private inner class ListAdapter(private val mContext: Context) :
        RecyclerListView.SelectionAdapter() {

        override fun getItemCount(): Int = rowCount

        override fun isEnabled(holder: RecyclerView.ViewHolder): Boolean =
            holder.itemViewType == 0

        override fun getItemViewType(position: Int): Int = if (position == emptyRow) 1 else 0

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                1 -> TextInfoPrivacyCell(mContext)
                else -> LogCell(mContext)
            }
            return RecyclerListView.Holder(view)
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when (holder.itemViewType) {
                0 -> {
                    val entry = entries.getOrNull(position) ?: return
                    (holder.itemView as LogCell).bind(entry)
                }
                1 -> {
                    val cell = holder.itemView as TextInfoPrivacyCell
                    cell.background = Theme.getThemedDrawableByKey(
                        mContext,
                        R.drawable.greydivider_bottom,
                        Theme.key_windowBackgroundGrayShadow
                    )
                    cell.setText(str(R.string.HaruLogsEmpty))
                }
            }
        }
    }

    // "12:04:31.220  message", timestamp gray and message in the entry's color
    private inner class LogCell(context: Context) : TextView(context) {

        init {
            // without this the row is only as wide as its text
            layoutParams = RecyclerView.LayoutParams(
                RecyclerView.LayoutParams.MATCH_PARENT,
                RecyclerView.LayoutParams.WRAP_CONTENT
            )
            setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 13f)
            typeface = Typeface.MONOSPACE
            setTextIsSelectable(false)
            setPadding(
                AndroidUtilities.dp(16f),
                AndroidUtilities.dp(6f),
                AndroidUtilities.dp(16f),
                AndroidUtilities.dp(6f)
            )
            gravity = if (LocaleController.isRTL) Gravity.RIGHT else Gravity.LEFT
            // Long lines wrap instead of being cut — a truncated log line is useless.
            ellipsize = null
            setHorizontallyScrolling(false)
        }

        fun bind(entry: HaruLog.Entry) {
            val stamp = timeFormat.format(entry.time)
            val builder = SpannableStringBuilder()
            builder.append(stamp).append("  ")
            builder.setSpan(
                ForegroundColorSpan(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText2)),
                0,
                builder.length,
                SpannableStringBuilder.SPAN_EXCLUSIVE_EXCLUSIVE
            )
            builder.append(entry.text)
            text = builder
            setTextColor(colorOf(entry.color))
            // A debug line stays readable but reads as secondary.
            alpha = if (entry.debug) 0.7f else 1f
        }

        private fun colorOf(color: HaruLog.Color): Int = Theme.getColor(
            when (color) {
                HaruLog.Color.GREEN -> Theme.key_windowBackgroundWhiteGreenText
                HaruLog.Color.YELLOW -> Theme.key_statisticChartLine_orange
                HaruLog.Color.RED -> Theme.key_text_RedRegular
                HaruLog.Color.DEFAULT -> Theme.key_windowBackgroundWhiteBlackText
            }
        )
    }
}
