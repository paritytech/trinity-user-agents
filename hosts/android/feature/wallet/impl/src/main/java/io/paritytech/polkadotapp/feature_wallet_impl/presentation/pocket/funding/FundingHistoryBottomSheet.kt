package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.fragment.app.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.FundingHistorySheetContent
import javax.inject.Inject

/**
 * The funding history opened from the funding sheet's amount screen, over it: Back leaves it for the amount screen
 * underneath, and a row opens its session over it.
 */
@AndroidEntryPoint
class FundingHistoryBottomSheet : BaseComposeBottomSheet<FundingHistoryViewModel>() {
    override val viewModel: FundingHistoryViewModel by viewModels()

    @Inject
    lateinit var tokenAmountFormatter: TokenAmountFormatter

    @Composable
    override fun Screen() {
        val state by viewModel.activity.collectAsStateWithLifecycle()

        CompositionLocalProvider(
            LocalTokenAmountFormatter provides tokenAmountFormatter,
        ) {
            NovaBottomSheetSurface {
                FundingHistorySheetContent(state = state, onBack = ::dismiss, onRowClick = viewModel::onRowClick)
            }
        }
    }
}
