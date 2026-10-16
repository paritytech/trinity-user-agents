package io.paritytech.polkadotapp.common.presentation.screens

import android.content.Context
import android.view.View
import androidx.activity.ComponentDialog
import androidx.activity.setViewTreeOnBackPressedDispatcherOwner
import androidx.annotation.StyleRes
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.R as RMaterial

abstract class BaseBottomSheetDialog(
    context: Context,
    @StyleRes theme: Int
) : BottomSheetDialog(context, theme) {
    init {
        // back handler fix for Compose
        // https://slack-chats.kotlinlang.org/t/12069403/hello-backhandler-doesn-t-work-inside-bottomsheetdialog-how-
        initViewTreeOwners()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        window?.let { it.decorView.systemUiVisibility = (it.decorView.systemUiVisibility or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION) }
        findViewById<View>(RMaterial.id.container)?.fitsSystemWindows = false
        findViewById<View>(RMaterial.id.coordinator)?.fitsSystemWindows = false
    }

    private fun ComponentDialog.initViewTreeOwners() {
        window?.decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeOnBackPressedDispatcherOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
        }
    }
}
