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
import org.telegram.ui.ActionBar.ActionBar
import org.telegram.ui.ActionBar.AlertDialog
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Cells.ShadowSectionCell
import org.telegram.ui.Cells.TextSettingsCell
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView

class Settings : BaseFragment() {

    companion object {
        private const val REQUEST_INSTALL_SDK = 7201
    }

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var installSdkRow = 0
    private var sectionRow = 1
    private var languageRow = 2
    private val rowCount = 3

    private fun str(resId: Int): String {
        val ctx = context ?: return ""
        return HaruLocale.getString(ctx, resId)
    }

    override fun createView(context: Context): View {
        actionBar.setBackButtonImage(R.drawable.ic_ab_back)
        actionBar.setAllowOverlayTitle(true)
        actionBar.setTitle(str(R.string.HaruSettings))
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
                    installSdkRow -> openFilePicker()
                    languageRow -> showLanguagePicker()
                }
            }
        }
        frameLayout.addView(listView, LayoutHelper.createFrameMatchParent())
        actionBar.setAdaptiveBackground(listView)

        return fragmentView
    }

    private fun openFilePicker() {
        val activity = parentActivity ?: return
        try {
            val intent = Intent(Intent.ACTION_GET_CONTENT).apply {
                type = "*/*"
                addCategory(Intent.CATEGORY_OPENABLE)
            }
            startActivityForResult(
                Intent.createChooser(intent, str(R.string.HaruChooseFile)),
                REQUEST_INSTALL_SDK
            )
        } catch (_: Exception) {
            try {
                activity.startActivityForResult(
                    Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = "*/*"
                    },
                    REQUEST_INSTALL_SDK
                )
            } catch (_: Exception) {
            }
        }
    }

    private fun showLanguagePicker() {
        val activity = parentActivity ?: return
        val codes = arrayOf(HaruLocale.LANG_EN, HaruLocale.LANG_RU, HaruLocale.LANG_DE)
        val labels = arrayOf(
            str(R.string.HaruLanguageEnglish),
            str(R.string.HaruLanguageRussian),
            str(R.string.HaruLanguageGerman)
        )
        val builder = AlertDialog.Builder(activity)
        builder.setTitle(str(R.string.HaruLanguage))
        builder.setItems(labels) { _, which ->
            if (which in codes.indices) {
                HaruLocale.setLanguage(codes[which])
                actionBar?.setTitle(str(R.string.HaruSettings))
                listAdapter?.notifyDataSetChanged()
            }
        }
        builder.setNegativeButton(str(android.R.string.cancel), null)
        showDialog(builder.create())
    }

    override fun onActivityResultFragment(requestCode: Int, resultCode: Int, data: Intent?) {
        // SDK file selected — no-op for now.
        if (requestCode == REQUEST_INSTALL_SDK && resultCode == Activity.RESULT_OK) {
            // intentionally empty
        }
    }

    override fun onResume() {
        super.onResume()
        actionBar?.setTitle(str(R.string.HaruSettings))
        listAdapter?.notifyDataSetChanged()
    }

    private inner class ListAdapter(private val mContext: Context) :
        RecyclerListView.SelectionAdapter() {

        override fun getItemCount(): Int = rowCount

        override fun isEnabled(holder: RecyclerView.ViewHolder): Boolean {
            return holder.itemViewType == 0
        }

        override fun getItemViewType(position: Int): Int {
            return if (position == sectionRow) 1 else 0
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                1 -> ShadowSectionCell(mContext)
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
                    when (position) {
                        installSdkRow -> cell.setText(str(R.string.HaruInstallSdk), true)
                        languageRow -> cell.setTextAndValue(
                            str(R.string.HaruLanguage),
                            HaruLocale.languageDisplayName(mContext, HaruLocale.getSavedLanguage()),
                            false
                        )
                    }
                }
            }
        }
    }
}
