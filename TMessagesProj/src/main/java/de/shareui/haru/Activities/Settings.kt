package de.shareui.haru.Activities

import android.app.Activity
import android.content.Context
import android.content.DialogInterface
import android.content.Intent
import android.graphics.drawable.GradientDrawable
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
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
import org.telegram.ui.Cells.ShadowSectionCell
import org.telegram.ui.Cells.TextCheckCell
import org.telegram.ui.Cells.TextSettingsCell
import org.telegram.ui.Components.EditTextBoldCursor
import org.telegram.ui.Components.LayoutHelper
import org.telegram.ui.Components.RecyclerListView

class Settings : BaseFragment() {
    companion object {
        private const val REQUEST_INSTALL_SDK = 7201
        private const val ID_SEPARATOR_MAX_LENGTH = 5
    }

    private var listView: RecyclerListView? = null
    private var listAdapter: ListAdapter? = null

    private var installSdkRow = -1
    private var sectionRow = -1
    private var showIdRow = -1
    private var idSeparatorRow = -1
    private var bottomSectionRow = -1
    private var rowCount = 0

    override fun onFragmentCreate(): Boolean {
        super.onFragmentCreate()
        updateRows()
        return true
    }

    private fun updateRows() {
        rowCount = 0
        installSdkRow = rowCount++
        sectionRow = rowCount++
        showIdRow = rowCount++
        idSeparatorRow = rowCount++
        bottomSectionRow = rowCount++
    }

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
            setOnItemClickListener { view, position ->
                when (position) {
                    installSdkRow -> openFilePicker()
                    showIdRow -> {
                        val enabled = !HaruLocale.isShowId()
                        HaruLocale.setShowId(enabled)
                        if (view is TextCheckCell) {
                            view.setChecked(enabled)
                        }
                    }
                    idSeparatorRow -> openIdSeparatorDialog()
                }
            }
        }
        frameLayout.addView(listView, LayoutHelper.createFrameMatchParent())
        actionBar.setAdaptiveBackground(listView)
        return fragmentView
    }

    private fun separatorDisplayValue(): String {
        val sep = HaruLocale.getIdSeparator()
        return if (sep.isEmpty()) str(R.string.HaruIdSeparatorNone) else sep
    }

    private fun openIdSeparatorDialog() {
        val activity = parentActivity ?: return
        val ctx = context ?: activity
        val resourcesProvider = resourceProvider

        val editText = EditTextBoldCursor(ctx).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_DIP, 16f)
            setTextColor(Theme.getColor(Theme.key_dialogTextBlack, resourcesProvider))
            setHintTextColor(Theme.getColor(Theme.key_groupcreate_hintText, resourcesProvider))
            hint = HaruLocale.DEFAULT_ID_SEPARATOR
            setFocusable(true)
            inputType = InputType.TYPE_CLASS_TEXT
            imeOptions = EditorInfo.IME_ACTION_DONE
            maxLines = 1
            setSingleLine(true)
            setPadding(
                AndroidUtilities.dp(16f),
                AndroidUtilities.dp(11f),
                AndroidUtilities.dp(16f),
                AndroidUtilities.dp(11f)
            )
            setCursorWidth(1.5f)
            setCursorColor(Theme.getColor(Theme.key_windowBackgroundWhiteBlueText4, resourcesProvider))
            setText(HaruLocale.getIdSeparator())
            setSelection(text?.length ?: 0)
            background = GradientDrawable().apply {
                cornerRadius = AndroidUtilities.dp(22f).toFloat()
                setColor(
                    Theme.multAlpha(
                        Theme.getColor(Theme.key_dialogTextBlack, resourcesProvider),
                        0.06f
                    )
                )
            }
        }

        editText.addTextChangedListener(object : TextWatcher {
            private var ignore = false
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                if (ignore || s == null) return
                if (s.length > ID_SEPARATOR_MAX_LENGTH) {
                    ignore = true
                    s.delete(ID_SEPARATOR_MAX_LENGTH, s.length)
                    AndroidUtilities.shakeView(editText)
                    ignore = false
                }
            }
        })

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

        val builder = AlertDialog.Builder(ctx, resourcesProvider)
            .setTitle(str(R.string.HaruIdSeparator))
            .setMessage(str(R.string.HaruIdSeparatorHint))
            .setView(container)
            .setPositiveButton(str(R.string.OK)) { dialog, _ ->
                // Keep raw text (no trim) so space can be a separator; empty = no grouping.
                val text = editText.text?.toString() ?: ""
                if (text.length > ID_SEPARATOR_MAX_LENGTH) {
                    AndroidUtilities.shakeView(editText)
                    return@setPositiveButton
                }
                HaruLocale.setIdSeparator(text)
                listAdapter?.notifyItemChanged(idSeparatorRow)
                dialog.dismiss()
            }
            .setNegativeButton(str(R.string.Cancel)) { dialog, _ ->
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
            if (actionId == EditorInfo.IME_ACTION_DONE) {
                val button = dialog.getButton(DialogInterface.BUTTON_POSITIVE)
                button?.performClick()
                true
            } else {
                false
            }
        }
        showDialog(dialog)
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
        // SDK file selected; TODO
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
            val type = holder.itemViewType
            return type == 0 || type == 3
        }

        override fun getItemViewType(position: Int): Int {
            return when (position) {
                sectionRow, bottomSectionRow -> 1
                showIdRow -> 3
                else -> 0
            }
        }

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
            val view: View = when (viewType) {
                3 -> TextCheckCell(mContext).apply {
                    setBackgroundColor(Theme.getColor(Theme.key_windowBackgroundWhite))
                }
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
                        installSdkRow -> {
                            cell.setText(str(R.string.HaruInstallSdk), false)
                            cell.setIcon(R.drawable.msg_download)
                        }
                        idSeparatorRow -> {
                            cell.setTextAndValue(
                                str(R.string.HaruIdSeparator),
                                separatorDisplayValue(),
                                false
                            )
                            cell.setIcon(0)
                        }
                    }
                }
                3 -> {
                    val cell = holder.itemView as TextCheckCell
                    cell.setTextAndCheck(
                        str(R.string.HaruShowId),
                        HaruLocale.isShowId(),
                        true
                    )
                }
            }
        }
    }
}
