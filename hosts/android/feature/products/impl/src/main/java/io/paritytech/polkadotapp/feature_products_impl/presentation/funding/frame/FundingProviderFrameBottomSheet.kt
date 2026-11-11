package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.ComposeView
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import dagger.hilt.android.lifecycle.withCreationCallback
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFrameContext
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFrameContexts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose.FundingProviderFrameScreen
import javax.inject.Inject

/**
 * The provider product's page, over the whole screen, hosted on the SPA host as any product page is.
 *
 * Brought back after process death, the frame has lost the call that opened it, so it closes at once; the core asks
 * for it again once the provider's worker is back. A frame answered while another screen sat on it closes once it is
 * back on top.
 */
@AndroidEntryPoint
class FundingProviderFrameBottomSheet : BaseComposeBottomSheet<FundingProviderFrameViewModel>() {
    override val viewModel: FundingProviderFrameViewModel by viewModels(
        extrasProducer = {
            defaultViewModelCreationExtras.withCreationCallback<FundingProviderFrameViewModel.Factory> { it.create(frame) }
        },
    )

    @Inject
    lateinit var frames: FundingFrameContexts

    // Looked up once, as the sheet is created, and handed to the view model as found: the overlay may drop it before the
    // view model is built, which happens only once a sheet that found it draws its screen.
    private lateinit var frame: FundingFrameContext

    private val orphaned: Boolean
        get() = !::frame.isInitialized

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        frames.get(arguments?.getString(INTENT).orEmpty())?.let { frame = it }
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
        when {
            orphaned -> Unit
            frame.isAnswered -> dismissAllowingStateLoss()
            else -> viewModel.resumeConnections()
        }
    }

    companion object {
        const val INTENT = "fundingFrameIntent"
    }
}
