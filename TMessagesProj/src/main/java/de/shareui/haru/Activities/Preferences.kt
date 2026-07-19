package de.shareui.haru.Activities

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import de.shareui.haru.HaruLocale
import org.telegram.messenger.R
import org.telegram.messenger.browser.Browser
import org.telegram.ui.ActionBar.ActionBar
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Cells.HeaderCell
import org.telegram.ui.Cells.ShadowSectionCell
import org.telegram.ui.Cells.TextSettingsCell
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView

class Preferences : BaseFragment() {

    companion object {
        private const val REQUEST_INSTALL_EXTENSION = 7101
    }

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var aboutRow = -1
    private var aboutSectionRow = -1
    private var navigationHeaderRow = -1
    private var installExtensionRow = -1
    private var extensionsListRow = -1
    private var debugMenuRow = -1
    private var haruSettingsRow = -1
    private var sectionRow = -1
    private var linksHeaderRow = -1
    private var telegramChannelRow = -1
    private var sourceCodeRow = -1
    private var bottomSectionRow = -1
    private var rowCount = 0

    override fun onFragmentCreate(): Boolean {
        super.onFragmentCreate()
        updateRows()
        return true
    }

    private fun updateRows() {
        rowCount = 0
        aboutRow = rowCount++
        aboutSectionRow = rowCount++
        navigationHeaderRow = rowCount++
        installExtensionRow = rowCount++
        extensionsListRow = rowCount++
        debugMenuRow = rowCount++
        haruSettingsRow = rowCount++
        sectionRow = rowCount++
        linksHeaderRow = rowCount++
        telegramChannelRow = rowCount++
        sourceCodeRow = rowCount++
        bottomSectionRow = rowCount++
    }

    private fun str(resId: Int): String {
        val ctx = context ?: return ""
        return HaruLocale.getString(ctx, resId)
    }

    override fun createView(context: Context): View {
        actionBar.setBackButtonImage(R.drawable.ic_ab_back)
        actionBar.setAllowOverlayTitle(true)
        actionBar.setTitle(str(R.string.HaruPreferences))
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
            setOnItemClickListener { _, position ->
                when (position) {
                    installExtensionRow -> openFilePicker(REQUEST_INSTALL_EXTENSION)
                    extensionsListRow -> presentFragment(Extensions())
                    debugMenuRow -> presentFragment(Debug())
                    haruSettingsRow -> presentFragment(Settings())
                    telegramChannelRow -> openUrl(str(R.string.HaruTelegramChannelUrl))
                    sourceCodeRow -> openUrl(str(R.string.HaruSourceCodeUrl))
                }
            }
        }
        frameLayout.addView(listView, LayoutHelper.createFrameMatchParent())
        actionBar.setAdaptiveBackground(listView)

        return fragmentView
    }

    private fun openUrl(url: String) {
        val activity = parentActivity ?: return
        if (url.isEmpty()) return
        Browser.openUrl(activity, url)
    }

    private fun openFilePicker(requestCode: Int) {
        val activity = parentActivity ?: return
        try {
            val intent = Intent(Intent.ACTION_GET_CONTENT).apply {
                type = "*/*"
                addCategory(Intent.CATEGORY_OPENABLE)
            }
            startActivityForResult(
                Intent.createChooser(intent, str(R.string.HaruChooseFile)),
                requestCode
            )
        } catch (_: Exception) {
            try {
                activity.startActivityForResult(
                    Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = "*/*"
                    },
                    requestCode
                )
            } catch (_: Exception) {
            }
        }
    }

    override fun onActivityResultFragment(requestCode: Int, resultCode: Int, data: Intent?) {
        // TODO
        if (requestCode == REQUEST_INSTALL_EXTENSION && resultCode == Activity.RESULT_OK) {
            // intentionally empty
        }
    }

    override fun onResume() {
        super.onResume()
        actionBar?.setTitle(str(R.string.HaruPreferences))
        listAdapter?.notifyDataSetChanged()
    }

    private inner class ListAdapter(private val mContext: Context) :
        RecyclerListView.SelectionAdapter() {

        override fun getItemCount(): Int = rowCount

        override fun isEnabled(holder: RecyclerView.ViewHolder): Boolean {
            val type = holder.itemViewType
            return type == 0
        }

        override fun getItemViewType(position: Int): Int {
            return when (position) {
                aboutRow -> 3
                aboutSectionRow, sectionRow, bottomSectionRow -> 1
                navigationHeaderRow, linksHeaderRow -> 2
                else -> 0
            }
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                3 -> HaruAboutCell(mContext)
                2 -> HeaderCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                }
                1 -> ShadowSectionCell(mContext)
                else -> TextSettingsCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                }
            }
            view.layoutParams = RecyclerView.LayoutParams(
                RecyclerView.LayoutParams.MATCH_PARENT,
                RecyclerView.LayoutParams.WRAP_CONTENT
            )
            return RecyclerListView.Holder(view)
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when (holder.itemViewType) {
                0 -> {
                    val cell = holder.itemView as TextSettingsCell
                    when (position) {
                        installExtensionRow -> {
                            cell.setText(str(R.string.HaruInstallExtension), true)
                            cell.setIcon(R.drawable.msg_download)
                        }
                        extensionsListRow -> {
                            cell.setText(str(R.string.HaruExtensionsList), false)
                            cell.setIcon(R.drawable.msg_plugins)
                        }
                        debugMenuRow -> {
                            cell.setText(str(R.string.HaruDebugMenu), false)
                            cell.setIcon(R.drawable.msg_log)
                        }
                        haruSettingsRow -> {
                            cell.setText(str(R.string.HaruSettings), false)
                            cell.setIcon(R.drawable.msg_settings)
                        }
                        telegramChannelRow -> {
                            cell.setTextAndValue(
                                str(R.string.HaruTelegramChannel),
                                str(R.string.HaruTelegramChannelValue),
                                true
                            )
                            cell.setIcon(R.drawable.msg_channel)
                        }
                        sourceCodeRow -> {
                            cell.setTextAndValue(
                                str(R.string.HaruSourceCode),
                                str(R.string.HaruSourceCodeValue),
                                false
                            )
                            cell.setIcon(R.drawable.msg_link2)
                        }
                    }
                }
                2 -> {
                    val cell = holder.itemView as HeaderCell
                    when (position) {
                        navigationHeaderRow -> cell.setText(str(R.string.HaruNavigation))
                        linksHeaderRow -> cell.setText(str(R.string.HaruLinks))
                    }
                }
                3 -> {
                    (holder.itemView as HaruAboutCell).bind()
                }
            }
        }
    }
}
