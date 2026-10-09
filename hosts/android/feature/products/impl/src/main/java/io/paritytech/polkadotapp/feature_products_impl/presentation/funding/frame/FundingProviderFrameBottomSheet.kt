package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose.FundingProviderFrameScreen

/** The provider product's page, over the whole screen, hosted the way the SPA sheet hosts a product. */
@AndroidEntryPoint
class FundingProviderFrameBottomSheet : BaseComposeBottomSheet<FundingProviderFrameViewModel>() {
    override val viewModel: FundingProviderFrameViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    @Composable
    override fun Screen() = FundingProviderFrameScreen(viewModel)

    override fun onPause() {
        super.onPause()
        viewModel.pauseConnections()
    }

    override fun onResume() {
        super.onResume()
        viewModel.resumeConnections()
    }

    companion object {
        const val INTENT = "fundingFrameIntent"
    }
}
