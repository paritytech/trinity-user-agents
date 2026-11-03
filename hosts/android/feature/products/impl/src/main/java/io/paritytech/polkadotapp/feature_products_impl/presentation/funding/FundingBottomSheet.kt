package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.ComposeView
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.FundingSheetScreen
import javax.inject.Inject

/**
 * The overlay a funding session is started from. Back walks the sheet's own screens; Close leaves it.
 *
 * Brought back after process death, the sheet has lost the call that opened it, so it closes at once. The session
 * itself lives on in the core, and the CASH card opens it again.
 */
@AndroidEntryPoint
class FundingBottomSheet : BaseComposeBottomSheet<FundingViewModel>() {
    override val viewModel: FundingViewModel by viewModels()

    @Inject
    lateinit var contexts: FundingOverlayContexts

    private val orphaned by lazy { contexts.get(arguments?.getString(INTENT).orEmpty()) == null }

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
    override fun Screen() = FundingSheetScreen(viewModel)

    companion object {
        const val INTENT = "fundingIntent"
    }
}
