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
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.progress.Shimmer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheetViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingAmountViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingCancelConfirmViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingCancelOfferViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingCountryViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingDepositViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingFeesViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingNetworkViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingProvidersViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingSummaryViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens.FundingTokenViewModel

private val SKELETON_FIGURE_HEIGHT = 64.dp
private val SKELETON_ROW_HEIGHT = 48.dp
private const val SKELETON_ROWS = 4

/** The screen on top of the sheet's stack, each with a view model of its own for that visit. */
@Composable
fun FundingSheetScreen(sheet: FundingSheetViewModel) {
    val entry by sheet.screen.collectAsStateWithLifecycle()

    BackHandler(onBack = sheet::onBack)

    NovaBottomSheetSurface {
        val contentModifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased)

        val current = entry
        val key = "funding-screen-${current?.key}"
        when (current?.screen) {
            null -> FundingSkeleton(modifier = contentModifier)
            FundingScreen.AMOUNT -> {
                val viewModel = hiltViewModel<FundingAmountViewModel, FundingAmountViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingAmountScreen(
                        modifier = contentModifier,
                        state = it,
                        onRailSelected = viewModel::onRailSelected,
                        onKey = viewModel::onKey,
                        onPreset = viewModel::onPreset,
                        onContinue = viewModel::onContinue,
                        onClose = viewModel::onClose,
                    )
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.SUMMARY -> {
                val viewModel = hiltViewModel<FundingSummaryViewModel, FundingSummaryViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingSummaryScreen(
                        modifier = contentModifier,
                        state = it,
                        onBack = sheet::onBack,
                        onFees = viewModel::onFees,
                        onCountry = viewModel::onCountry,
                        onProviders = viewModel::onProviders,
                        onContinue = viewModel::onStart,
                    )
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.FEES -> {
                val viewModel = hiltViewModel<FundingFeesViewModel, FundingFeesViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                FundingFeesScreen(modifier = contentModifier, state = state, onBack = sheet::onBack)
            }

            FundingScreen.COUNTRY -> {
                val viewModel = hiltViewModel<FundingCountryViewModel, FundingCountryViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingCountryScreen(
                        modifier = contentModifier,
                        state = it,
                        onBack = sheet::onBack,
                        onQueryChanged = viewModel::onQueryChanged,
                        onCountry = viewModel::onCountryChosen,
                    )
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.PROVIDERS -> {
                val viewModel = hiltViewModel<FundingProvidersViewModel, FundingProvidersViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingProvidersScreen(modifier = contentModifier, state = it, onBack = sheet::onBack, onProvider = viewModel::onProviderChosen)
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.NETWORK -> {
                val viewModel = hiltViewModel<FundingNetworkViewModel, FundingNetworkViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingNetworkScreen(modifier = contentModifier, rows = it, onBack = sheet::onBack, onNetwork = viewModel::onNetworkChosen)
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.TOKEN -> {
                val viewModel = hiltViewModel<FundingTokenViewModel, FundingTokenViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingTokenScreen(modifier = contentModifier, state = it, onBack = sheet::onBack, onToken = viewModel::onTokenChosen)
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.DEPOSIT -> {
                val viewModel = hiltViewModel<FundingDepositViewModel, FundingDepositViewModel.Factory>(key = key) { it.create(sheet) }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingDepositScreen(
                        modifier = contentModifier,
                        state = it,
                        onBack = sheet::onBack,
                        onCopy = viewModel::onCopy,
                        onCancel = viewModel::onCancel,
                        onReviewOffer = viewModel::onReviewOffer,
                        onClose = viewModel::onClose,
                    )
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.CANCEL_CONFIRM -> {
                val viewModel = hiltViewModel<FundingCancelConfirmViewModel, FundingCancelConfirmViewModel.Factory>(key = key) {
                    it.create(sheet)
                }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingCancelConfirmScreen(modifier = contentModifier, state = it, onBack = sheet::onBack, onConfirm = viewModel::onConfirm)
                } ?: FundingSkeleton(contentModifier)
            }

            FundingScreen.CANCEL_OFFER -> {
                val viewModel = hiltViewModel<FundingCancelOfferViewModel, FundingCancelOfferViewModel.Factory>(key = key) {
                    it.create(sheet)
                }
                val state by viewModel.state.collectAsStateWithLifecycle()
                state?.let {
                    FundingCancelOfferScreen(
                        modifier = contentModifier,
                        offer = it.offer,
                        withdraw = it.withdraw,
                        isAnswering = it.isAnswering,
                        onBack = sheet::onBack,
                        onAccept = viewModel::onAccept,
                        onDecline = viewModel::onDecline,
                    )
                }
            }
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
