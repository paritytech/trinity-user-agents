package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.FundingSheetScreen

@AndroidEntryPoint
class FundingBottomSheet : BaseComposeBottomSheet<FundingSheetViewModel>() {
    override val viewModel: FundingSheetViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    override fun onResume() {
        super.onResume()
        if (viewModel.isClosed) dismissAllowingStateLoss()
    }

    @Composable
    override fun Screen() = FundingSheetScreen(viewModel)

    companion object {
        const val SESSION_ID = "fundingSessionId"
    }
}
