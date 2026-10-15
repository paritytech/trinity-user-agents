package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.progress.Shimmer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCancelUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingViewModel

private val SKELETON_FIGURE_HEIGHT = 64.dp
private val SKELETON_ROW_HEIGHT = 48.dp
private const val SKELETON_ROWS = 4

@Composable
fun FundingSheetScreen(viewModel: FundingViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    BackHandler(onBack = viewModel::onBack)

    NovaBottomSheetSurface {
        val contentModifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased)

        when (val current = state) {
            is LoadingState.Loaded -> {
                when (current.data.screen) {
                    FundingScreen.SUMMARY -> FundingSummaryScreen(
                        modifier = contentModifier,
                        state = current.data.summary,
                        onBack = viewModel::onBack,
                        onFees = viewModel::onOpenFees,
                        onCountry = viewModel::onOpenCountry,
                        onProviders = viewModel::onOpenProviders,
                        onContinue = viewModel::onStart,
                    )

                    FundingScreen.FEES -> FundingFeesScreen(modifier = contentModifier, state = current.data.fees, onBack = viewModel::onBack)

                    FundingScreen.COUNTRY -> FundingCountryScreen(
                        modifier = contentModifier,
                        state = current.data.country,
                        onBack = viewModel::onBack,
                        onQueryChanged = viewModel::onCountryQueryChanged,
                        onCountry = viewModel::onCountryChosen,
                    )

                    FundingScreen.PROVIDERS -> FundingProvidersScreen(
                        modifier = contentModifier,
                        state = current.data.providers,
                        onBack = viewModel::onBack,
                        onProvider = viewModel::onProviderChosen,
                    )

                    FundingScreen.NETWORK -> FundingNetworkScreen(
                        modifier = contentModifier,
                        rows = current.data.networks,
                        onBack = viewModel::onBack,
                        onNetwork = viewModel::onNetworkChosen,
                    )

                    FundingScreen.TOKEN -> FundingTokenScreen(
                        modifier = contentModifier,
                        state = current.data.tokens,
                        onBack = viewModel::onBack,
                        onToken = viewModel::onTokenChosen,
                    )

                    FundingScreen.DEPOSIT -> FundingDepositScreen(
                        modifier = contentModifier,
                        state = current.data.deposit,
                        onBack = viewModel::onBack,
                        onCopy = viewModel::onCopy,
                        onCancel = viewModel::onCancelTopUp,
                        onReviewOffer = viewModel::onReviewCancelOffer,
                        onClose = viewModel::onClose,
                    )

                    FundingScreen.CANCEL_CONFIRM -> FundingCancelConfirmScreen(
                        modifier = contentModifier,
                        state = current.data.deposit,
                        onBack = viewModel::onBack,
                        onConfirm = viewModel::onConfirmCancel,
                    )

                    FundingScreen.CANCEL_OFFER -> (current.data.deposit.cancel as? FundingCancelUiState.Offered)?.let { offer ->
                        FundingCancelOfferScreen(
                            modifier = contentModifier,
                            offer = offer,
                            isAnswering = current.data.deposit.isCancelling,
                            onBack = viewModel::onBack,
                            onAccept = viewModel::onAcceptCancelOffer,
                            onDecline = viewModel::onDeclineCancelOffer,
                        )
                    }

                    FundingScreen.AMOUNT -> FundingAmountScreen(
                        modifier = contentModifier,
                        state = current.data.amount,
                        onRailSelected = viewModel::onRailSelected,
                        onKey = viewModel::onKey,
                        onPreset = viewModel::onPreset,
                        onContinue = viewModel::onContinueFromAmount,
                        onClose = viewModel::onClose,
                    )
                }
            }

            else -> FundingSkeleton(modifier = contentModifier)
        }
    }
}

@Composable
private fun FundingSkeleton(modifier: Modifier = Modifier) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        Shimmer(modifier = Modifier.fillMaxWidth().height(SKELETON_FIGURE_HEIGHT))
        repeat(SKELETON_ROWS) {
            Shimmer(modifier = Modifier.fillMaxWidth().height(SKELETON_ROW_HEIGHT))
        }
    }
}
