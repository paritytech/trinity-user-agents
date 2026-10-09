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
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheetUiState
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
            is LoadingState.Loaded -> FundingSheetContent(modifier = contentModifier, state = current.data, viewModel = viewModel)
            else -> FundingSkeleton(modifier = contentModifier)
        }
    }
}

@Composable
private fun FundingSheetContent(
    state: FundingSheetUiState,
    viewModel: FundingViewModel,
    modifier: Modifier = Modifier,
) {
    when (state.screen) {
        FundingScreen.SUMMARY -> FundingSummaryScreen(
            modifier = modifier,
            state = state.summary,
            onBack = viewModel::onBack,
            onFees = viewModel::onOpenFees,
            onCountry = viewModel::onOpenCountry,
            onProviders = viewModel::onOpenProviders,
            onContinue = viewModel::onStart,
        )

        FundingScreen.FEES -> FundingFeesScreen(modifier = modifier, state = state.fees, onBack = viewModel::onBack)

        FundingScreen.COUNTRY -> FundingCountryScreen(
            modifier = modifier,
            state = state.country,
            onBack = viewModel::onBack,
            onQueryChanged = viewModel::onCountryQueryChanged,
            onCountry = viewModel::onCountryChosen,
        )

        FundingScreen.PROVIDERS -> FundingProvidersScreen(
            modifier = modifier,
            state = state.providers,
            onBack = viewModel::onBack,
            onProvider = viewModel::onProviderChosen,
        )

        FundingScreen.AMOUNT,
        FundingScreen.NETWORK,
        FundingScreen.TOKEN,
        FundingScreen.DEPOSIT,
        FundingScreen.CANCEL_CONFIRM -> FundingAmountScreen(
            modifier = modifier,
            state = state.amount,
            onRailSelected = viewModel::onRailSelected,
            onKey = viewModel::onKey,
            onPreset = viewModel::onPreset,
            onContinue = viewModel::onContinueFromAmount,
            onClose = viewModel::onClose,
        )
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
