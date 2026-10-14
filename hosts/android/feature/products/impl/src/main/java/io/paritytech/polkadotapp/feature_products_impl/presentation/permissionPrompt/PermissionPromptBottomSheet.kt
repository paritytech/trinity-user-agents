package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.PermissionPromptScreen

@AndroidEntryPoint
class PermissionPromptBottomSheet : BaseComposeBottomSheet<PermissionPromptViewModel>() {
    override val viewModel: PermissionPromptViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    override fun onResume() {
        super.onResume()
        viewModel.onResume()
    }

    @Composable
    override fun Screen() = PermissionPromptScreen(viewModel)

    companion object {
        const val REQUEST_ID = "permissionRequestId"
    }
}
