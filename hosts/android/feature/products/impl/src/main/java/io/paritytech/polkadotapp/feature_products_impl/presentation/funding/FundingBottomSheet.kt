package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.FundingSheetScreen

/** The overlay a funding session is started from. Back walks the sheet's own screens; Close leaves it. */
@AndroidEntryPoint
class FundingBottomSheet : BaseComposeBottomSheet<FundingViewModel>() {
    override val viewModel: FundingViewModel by viewModels()

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
