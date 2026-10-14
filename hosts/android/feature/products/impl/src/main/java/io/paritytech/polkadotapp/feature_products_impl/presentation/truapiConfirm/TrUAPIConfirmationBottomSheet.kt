package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.compose.TrUAPIConfirmationScreen

@AndroidEntryPoint
class TrUAPIConfirmationBottomSheet : BaseComposeBottomSheet<TrUAPIConfirmationViewModel>() {
    override val viewModel: TrUAPIConfirmationViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    @Composable
    override fun Screen() {
        TrUAPIConfirmationScreen(viewModel)
    }
}
