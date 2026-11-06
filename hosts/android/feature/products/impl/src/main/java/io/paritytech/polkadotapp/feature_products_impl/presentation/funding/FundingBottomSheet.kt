package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

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
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.FundingSheetScreen
import javax.inject.Inject

/**
 * The overlay a funding session is started from. Back walks the sheet's own screens; Close leaves it.
 *
 * Brought back after process death, the sheet has lost the call that opened it, so it closes at once. The session
 * itself lives on in the core, and the CASH card opens it again. A sheet asked to close while another screen sat on it
 * closes once it is back on top.
 */
@AndroidEntryPoint
class FundingBottomSheet : BaseComposeBottomSheet<FundingViewModel>() {
    override val viewModel: FundingViewModel by viewModels(
        extrasProducer = {
            defaultViewModelCreationExtras.withCreationCallback<FundingViewModel.Factory> { it.create(overlay) }
        },
    )

    @Inject
    lateinit var contexts: FundingOverlayContexts

    // Looked up once, as the sheet is created, and handed to the view model as found: the overlay may drop it before the
    // view model is built, which happens only once a sheet that found it draws its screen.
    private lateinit var overlay: FundingOverlayContext

    private val orphaned: Boolean
        get() = !::overlay.isInitialized

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        contexts.get(arguments?.getString(INTENT).orEmpty())?.let { overlay = it }
        if (orphaned) dismissAllowingStateLoss() else overlay.markShown()
    }

    override fun onResume() {
        super.onResume()
        if (!orphaned && overlay.isClosed) dismissAllowingStateLoss()
    }

    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): ComposeView =
        if (orphaned) ComposeView(requireContext()) else super.onCreateView(inflater, container, savedInstanceState)

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    @Composable
    override fun Screen() = FundingSheetScreen(viewModel)

    companion object {
        const val INTENT = "fundingIntent"
    }
}
