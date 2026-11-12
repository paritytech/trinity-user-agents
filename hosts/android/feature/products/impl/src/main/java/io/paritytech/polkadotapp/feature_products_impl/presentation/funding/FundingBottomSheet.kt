package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.FundingSheetScreen

/**
 * The sheet a funding session is shown in, found by its [INTENT]. Back walks the sheet's own screens; Close leaves it.
 * It follows the session through the funding service, and closes itself once the session is gone, as after process
 * death for one that never started.
 */
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
        const val INTENT = "fundingIntent"
    }
}
