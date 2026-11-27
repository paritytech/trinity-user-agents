package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.ComposeView
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose.FundingProviderFrameScreen

// After process death no frame waits under it, so it closes at once; the core asks for the screen again once the
// provider's worker is back.
@AndroidEntryPoint
class FundingProviderFrameBottomSheet : BaseComposeBottomSheet<FundingProviderFrameViewModel>() {
    override val viewModel: FundingProviderFrameViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (viewModel.isOrphaned) dismissAllowingStateLoss()
    }

    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): ComposeView =
        if (viewModel.isOrphaned) ComposeView(requireContext()) else super.onCreateView(inflater, container, savedInstanceState)

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
        if (viewModel.isAnswered) dismissAllowingStateLoss() else viewModel.resumeConnections()
    }

    companion object {
        const val SESSION_ID = "fundingFrameSessionId"
        const val REQUEST_ID = "fundingFrameRequestId"
    }
}
