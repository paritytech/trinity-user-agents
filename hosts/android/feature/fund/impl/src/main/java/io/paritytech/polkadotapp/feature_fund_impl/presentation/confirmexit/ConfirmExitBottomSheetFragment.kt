package io.paritytech.polkadotapp.feature_fund_impl.presentation.confirmexit

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_fund_impl.presentation.confirmexit.compose.ConfirmExitScreen

@AndroidEntryPoint
class ConfirmExitBottomSheetFragment :
    BaseComposeBottomSheet<ConfirmExitBottomSheetViewModel>() {
    override val viewModel: ConfirmExitBottomSheetViewModel by viewModels()

    @Composable
    override fun Screen() = ConfirmExitScreen(viewModel)
}
