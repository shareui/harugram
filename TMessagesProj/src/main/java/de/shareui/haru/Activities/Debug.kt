package de.shareui.haru.Activities

import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import de.shareui.haru.HaruLocale
import org.telegram.messenger.R
import org.telegram.ui.ActionBar.ActionBar
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Cells.TextCheckCell
import org.telegram.ui.Cells.TextSettingsCell
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView

class Debug : BaseFragment() {

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var openLogsRow = 0
    private var verboseLoggingRow = 1
    private val rowCount = 2

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

        listAdapter = ListAdapter(context)
        fragmentView = FrameLayout(context).apply {
            setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundGray))
        }
        val frameLayout = fragmentView as FrameLayout

        listView = RecyclerListView(context).apply {
            setSections()
            setVerticalScrollBarEnabled(false)
            layoutManager = LinearLayoutManager(context, LinearLayoutManager.VERTICAL, false)
            adapter = listAdapter
            setOnItemClickListener { view, position ->
                when (position) {
                    openLogsRow -> {
                        // no-op for now
                    }
                    verboseLoggingRow -> {
                        // switch is visual only for now; still persist state so UI stays consistent
                        val enabled = !HaruLocale.isVerboseLogging()
                        HaruLocale.setVerboseLogging(enabled)
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

        override fun isEnabled(holder: RecyclerView.ViewHolder): Boolean = true

        override fun getItemViewType(position: Int): Int {
            return if (position == verboseLoggingRow) 1 else 0
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                1 -> TextCheckCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
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
                    cell.setText(str(R.string.HaruOpenLogs), true)
                }
                1 -> {
                    val cell = holder.itemView as TextCheckCell
                    cell.setTextAndCheck(
                        str(R.string.HaruVerboseLogging),
                        HaruLocale.isVerboseLogging(),
                        false
                    )
                }
            }
        }
    }
}
