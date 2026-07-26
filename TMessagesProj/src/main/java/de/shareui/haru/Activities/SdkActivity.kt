package de.shareui.haru.Activities

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import de.shareui.haru.HaruLocale
import de.shareui.haru.Sdk.HaruSdk
import de.shareui.haru.Sdk.SdkManager
import org.telegram.messenger.AndroidUtilities
import org.telegram.messenger.LocaleController
import org.telegram.messenger.R
import org.telegram.messenger.Utilities
import org.telegram.ui.ActionBar.ActionBar
import org.telegram.ui.ActionBar.AlertDialog
import org.telegram.ui.ActionBar.BaseFragment
import org.telegram.ui.ActionBar.Theme
import org.telegram.ui.Cells.TextInfoPrivacyCell
import org.telegram.ui.Components.BulletinFactory
import org.telegram.ui.Components.ItemOptions
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView
import org.telegram.ui.Components.Switch

/**
 * List of installed SDKs. Each one is a container card carrying its metadata
 * and a switch; long press opens delete. Installing is available both here and
 * from [Settings].
 */
class SdkActivity : BaseFragment() {

    companion object {
        const val REQUEST_INSTALL_SDK = 7202
        private const val MENU_INSTALL = 1
    }

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var sdks: List<HaruSdk> = emptyList()

    private var infoRow = -1
    private var rowCount = 0

    override fun onFragmentCreate(): Boolean {
        super.onFragmentCreate()
        reload()
        return true
    }

    private fun reload() {
        sdks = SdkManager.list()
        rowCount = sdks.size
        infoRow = rowCount++
    }

    private fun str(resId: Int): String {
        val ctx = context ?: return ""
        return HaruLocale.getString(ctx, resId)
    }

    private fun str(resId: Int, vararg args: Any): String {
        val ctx = context ?: return ""
        return HaruLocale.getString(ctx, resId, *args)
    }

    override fun createView(context: Context): View {
        actionBar.setBackButtonImage(R.drawable.ic_ab_back)
        actionBar.setAllowOverlayTitle(true)
        actionBar.setTitle(str(R.string.HaruSdkList))
        actionBar.setActionBarMenuOnItemClick(object : ActionBar.ActionBarMenuOnItemClick() {
            override fun onItemClick(id: Int) {
                when (id) {
                    -1 -> finishFragment()
                    MENU_INSTALL -> openFilePicker()
                }
            }
        })
        actionBar.createMenu().addItem(MENU_INSTALL, R.drawable.msg_download)

        listAdapter = ListAdapter(context)
        fragmentView = FrameLayout(context).apply {
            setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundGray))
        }
        val frameLayout = fragmentView as FrameLayout

        listView = RecyclerListView(context).apply {
            setVerticalScrollBarEnabled(false)
            layoutManager = LinearLayoutManager(context, LinearLayoutManager.VERTICAL, false)
            adapter = listAdapter
            setOnItemClickListener { view, position ->
                val sdk = sdks.getOrNull(position) ?: return@setOnItemClickListener
                toggle(sdk, view as? SdkCell)
            }
            setOnItemLongClickListener(RecyclerListView.OnItemLongClickListener { view, position ->
                val sdk = sdks.getOrNull(position)
                    ?: return@OnItemLongClickListener false
                showOptions(sdk, view)
                true
            })
        }
        frameLayout.addView(listView, LayoutHelper.createFrameMatchParent())
        actionBar.setAdaptiveBackground(listView)

        return fragmentView
    }

    override fun onResume() {
        super.onResume()
        actionBar?.setTitle(str(R.string.HaruSdkList))
        refreshList()
    }

    private fun refreshList() {
        reload()
        listAdapter?.notifyDataSetChanged()
    }

    // region actions

    private fun toggle(sdk: HaruSdk, cell: SdkCell?) {
        val enable = !SdkManager.isEnabled(sdk.id)
        val error = SdkManager.setEnabled(sdk, enable)
        cell?.bind(sdk)

        if (error != null) {
            // The flag is on but the dex refused to load — say so instead of
            // leaving a switch that looks fine.
            BulletinFactory.of(this)
                .createErrorBulletin(str(R.string.HaruSdkErrorStart, sdk.name))
                .show()
        } else if (!enable) {
            BulletinFactory.of(this)
                .createSimpleBulletin(R.raw.info, str(R.string.HaruSdkRestartToUnload, sdk.name))
                .show()
        }
    }

    private fun showOptions(sdk: HaruSdk, anchor: View) {
        ItemOptions.makeOptions(this, anchor)
            .add(R.drawable.msg_delete, str(R.string.HaruSdkDelete), true) { confirmDelete(sdk) }
            .setGravity(if (LocaleController.isRTL) Gravity.LEFT else Gravity.RIGHT)
            .show()
    }

    private fun confirmDelete(sdk: HaruSdk) {
        val ctx = context ?: return
        val builder = AlertDialog.Builder(ctx, resourceProvider)
            .setTitle(str(R.string.HaruSdkDeleteTitle))
            .setMessage(str(R.string.HaruSdkDeleteMessage, sdk.name))
            .setPositiveButton(str(R.string.HaruSdkDelete)) { dialog, _ ->
                dialog.dismiss()
                delete(sdk)
            }
            .setNegativeButton(str(R.string.Cancel)) { dialog, _ -> dialog.dismiss() }
        showDialog(builder.create())
    }

    private fun delete(sdk: HaruSdk) {
        val removed = SdkManager.uninstall(sdk)
        refreshList()
        val message = if (removed) {
            str(R.string.HaruSdkRemoved, sdk.name)
        } else {
            str(R.string.HaruSdkErrorInstall)
        }
        BulletinFactory.of(this).createSimpleBulletin(R.raw.info, message).show()
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

    override fun onActivityResultFragment(requestCode: Int, resultCode: Int, data: Intent?) {
        if (requestCode != REQUEST_INSTALL_SDK || resultCode != Activity.RESULT_OK) {
            return
        }
        val uri = data?.data ?: return
        install(this, uri) { refreshList() }
    }

    // endregion

    private inner class ListAdapter(private val mContext: Context) :
        RecyclerListView.SelectionAdapter() {

        override fun getItemCount(): Int = rowCount

        override fun isEnabled(holder: RecyclerView.ViewHolder): Boolean =
            holder.itemViewType == 0

        override fun getItemViewType(position: Int): Int = if (position == infoRow) 1 else 0

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                1 -> TextInfoPrivacyCell(mContext)
                else -> SdkCell(mContext)
            }
            if (viewType == 0) {
                view.layoutParams = RecyclerView.LayoutParams(
                    RecyclerView.LayoutParams.MATCH_PARENT,
                    RecyclerView.LayoutParams.WRAP_CONTENT
                ).apply {
                    leftMargin = AndroidUtilities.dp(12f)
                    rightMargin = AndroidUtilities.dp(12f)
                    topMargin = AndroidUtilities.dp(6f)
                    bottomMargin = AndroidUtilities.dp(6f)
                }
            }
            return RecyclerListView.Holder(view)
        }

        override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
            when (holder.itemViewType) {
                0 -> {
                    val sdk = sdks.getOrNull(position) ?: return
                    (holder.itemView as SdkCell).bind(sdk)
                }
                1 -> {
                    val cell = holder.itemView as TextInfoPrivacyCell
                    cell.background = Theme.getThemedDrawableByKey(
                        mContext,
                        R.drawable.greydivider_bottom,
                        Theme.key_windowBackgroundGrayShadow
                    )
                    cell.setText(
                        if (sdks.isEmpty()) str(R.string.HaruSdkEmpty) else str(R.string.HaruSdkInfo)
                    )
                }
            }
        }
    }

    /** exteraGram-style container: metadata block on one side, switch on the other. */
    private inner class SdkCell(context: Context) : FrameLayout(context) {

        private val nameView: TextView
        private val infoView: TextView
        private val idView: TextView
        private val switchView: Switch

        init {
            background = Theme.createSimpleSelectorRoundRectDrawable(
                AndroidUtilities.dp(12f),
                Theme.getColor(Theme.key_windowBackgroundWhite),
                Theme.getColor(Theme.key_listSelector)
            )

            val texts = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }

            nameView = TextView(context).apply {
                setTextSize(TypedValue.COMPLEX_UNIT_DIP, 16f)
                setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlackText))
                typeface = AndroidUtilities.bold()
                setLines(1)
                maxLines = 1
                setSingleLine(true)
                ellipsize = android.text.TextUtils.TruncateAt.END
                gravity = if (LocaleController.isRTL) Gravity.RIGHT else Gravity.LEFT
            }
            infoView = TextView(context).apply {
                setTextSize(TypedValue.COMPLEX_UNIT_DIP, 13f)
                setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText2))
                maxLines = 2
                ellipsize = android.text.TextUtils.TruncateAt.END
                gravity = if (LocaleController.isRTL) Gravity.RIGHT else Gravity.LEFT
            }
            idView = TextView(context).apply {
                setTextSize(TypedValue.COMPLEX_UNIT_DIP, 12f)
                setTextColor(Theme.getColor(Theme.key_windowBackgroundWhiteGrayText2))
                setLines(1)
                maxLines = 1
                setSingleLine(true)
                ellipsize = android.text.TextUtils.TruncateAt.MIDDLE
                gravity = if (LocaleController.isRTL) Gravity.RIGHT else Gravity.LEFT
            }

            texts.addView(nameView, LayoutHelper.createLinear(LayoutHelper.MATCH_PARENT, LayoutHelper.WRAP_CONTENT))
            texts.addView(infoView, LayoutHelper.createLinear(LayoutHelper.MATCH_PARENT, LayoutHelper.WRAP_CONTENT, 0f, 3f, 0f, 0f))
            texts.addView(idView, LayoutHelper.createLinear(LayoutHelper.MATCH_PARENT, LayoutHelper.WRAP_CONTENT, 0f, 2f, 0f, 0f))

            addView(
                texts,
                LayoutHelper.createFrame(
                    LayoutHelper.MATCH_PARENT,
                    LayoutHelper.WRAP_CONTENT.toFloat(),
                    (if (LocaleController.isRTL) Gravity.RIGHT else Gravity.LEFT) or Gravity.CENTER_VERTICAL,
                    if (LocaleController.isRTL) 66f else 16f,
                    14f,
                    if (LocaleController.isRTL) 16f else 66f,
                    14f
                )
            )

            switchView = Switch(context).apply {
                setColors(
                    Theme.key_switchTrack,
                    Theme.key_switchTrackChecked,
                    Theme.key_windowBackgroundWhite,
                    Theme.key_windowBackgroundWhite
                )
            }
            addView(
                switchView,
                LayoutHelper.createFrame(
                    37, 20f,
                    (if (LocaleController.isRTL) Gravity.LEFT else Gravity.RIGHT) or Gravity.CENTER_VERTICAL,
                    if (LocaleController.isRTL) 16f else 0f,
                    0f,
                    if (LocaleController.isRTL) 0f else 16f,
                    0f
                )
            )
        }

        fun bind(sdk: HaruSdk) {
            nameView.text = sdk.name
            infoView.text = describe(sdk)
            idView.text = sdk.id
            switchView.setChecked(SdkManager.isEnabled(sdk.id), true)
        }

        /** `by haru · v0.1.0 · alpha`, skipping whatever the metadata leaves out. */
        private fun describe(sdk: HaruSdk): String {
            val parts = ArrayList<String>(3)
            parts.add(
                if (sdk.author.isNotEmpty()) {
                    str(R.string.HaruSdkByAuthor, sdk.author)
                } else {
                    str(R.string.HaruSdkNoAuthor)
                }
            )
            if (sdk.version.isNotEmpty()) {
                parts.add("v${sdk.version}")
            }
            if (sdk.state.isNotEmpty()) {
                parts.add(sdk.state)
            }
            if (!SdkManager.isEnabled(sdk.id)) {
                parts.add(str(R.string.HaruSdkDisabled))
            } else if (!SdkManager.isRunning(sdk.id)) {
                parts.add(str(R.string.HaruSdkNotLoaded))
            }
            return parts.joinToString(" · ")
        }
    }
}

/**
 * Installs the picked archive off the main thread and reports the outcome.
 * Shared by [SdkActivity] and [Settings], which both offer the install button.
 */
internal fun install(fragment: BaseFragment, uri: Uri, onDone: () -> Unit) {
    val context = fragment.context ?: fragment.parentActivity ?: return
    val progress = AlertDialog(context, AlertDialog.ALERT_TYPE_SPINNER)
    progress.show()

    Utilities.globalQueue.postRunnable {
        val result = SdkManager.install(context, uri)
        AndroidUtilities.runOnUIThread {
            try {
                progress.dismiss()
            } catch (_: Exception) {
            }
            onDone()

            val factory = BulletinFactory.of(fragment)
            when (result) {
                is SdkManager.InstallResult.Success -> {
                    val text = HaruLocale.getString(
                        context,
                        if (result.replaced) R.string.HaruSdkUpdated else R.string.HaruSdkInstalled,
                        result.sdk.name
                    )
                    factory.createSimpleBulletin(R.raw.contact_check, text).show()
                    // Freshly installed SDKs default to enabled, so load it right away.
                    if (SdkManager.isEnabled(result.sdk.id)) {
                        val error = SdkManager.start(result.sdk)
                        if (error != null) {
                            BulletinFactory.of(fragment).createErrorBulletin(
                                HaruLocale.getString(context, R.string.HaruSdkErrorStart, result.sdk.name)
                            ).show()
                        }
                        onDone()
                    }
                }
                is SdkManager.InstallResult.Error -> {
                    val resId = when (result.failure) {
                        SdkManager.Failure.UNREADABLE -> R.string.HaruSdkErrorRead
                        SdkManager.Failure.NOT_AN_ARCHIVE -> R.string.HaruSdkErrorArchive
                        SdkManager.Failure.NO_MANIFEST -> R.string.HaruSdkErrorManifest
                        SdkManager.Failure.NO_DEX -> R.string.HaruSdkErrorDex
                        SdkManager.Failure.IO -> R.string.HaruSdkErrorInstall
                    }
                    factory.createErrorBulletin(HaruLocale.getString(context, resId)).show()
                }
            }
        }
    }
}
