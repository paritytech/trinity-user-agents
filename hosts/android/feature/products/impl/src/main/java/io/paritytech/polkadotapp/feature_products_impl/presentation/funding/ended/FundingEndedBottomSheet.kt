package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet

/** An ended session's detail, read from the host's history, so it also comes back after process death. */
@AndroidEntryPoint
class FundingEndedBottomSheet : BaseComposeBottomSheet<FundingEndedViewModel>() {
    override val viewModel: FundingEndedViewModel by viewModels()

    @Composable
    override fun Screen() = FundingEndedScreen(viewModel)

    companion object {
        const val INTENT = "fundingEndedIntent"
    }
}
