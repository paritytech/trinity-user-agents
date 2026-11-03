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
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameContexts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose.FundingProviderFrameScreen
import javax.inject.Inject

/**
 * The provider product's page, over the whole screen, hosted the way the SPA sheet hosts a product.
 *
 * Brought back after process death, the frame has lost the call that opened it, so it closes at once; the core asks
 * for it again once the provider's worker is back.
 */
@AndroidEntryPoint
class FundingProviderFrameBottomSheet : BaseComposeBottomSheet<FundingProviderFrameViewModel>() {
    override val viewModel: FundingProviderFrameViewModel by viewModels()

    @Inject
    lateinit var frames: FundingFrameContexts

    private val orphaned by lazy { frames.get(arguments?.getString(INTENT).orEmpty()) == null }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (orphaned) dismissAllowingStateLoss()
    }

    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): ComposeView =
        if (orphaned) ComposeView(requireContext()) else super.onCreateView(inflater, container, savedInstanceState)

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    @Composable
    override fun Screen() = FundingProviderFrameScreen(viewModel)

    override fun onPause() {
        super.onPause()
        if (!orphaned) viewModel.pauseConnections()
    }

    override fun onResume() {
        super.onResume()
        if (!orphaned) viewModel.resumeConnections()
    }

    companion object {
        const val INTENT = "fundingFrameIntent"
    }
}
