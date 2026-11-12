package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSessionFacts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingStartState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSummaryUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toSummaryUiState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn

/** The quote the session starts on, and the start itself. Where the sheet goes once it started follows the session. */
@HiltViewModel(assistedFactory = FundingSummaryViewModel.Factory::class)
class FundingSummaryViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
    private val service: FundingService,
) : BaseViewModel() {
    private val start = MutableStateFlow<FundingStartState>(FundingStartState.Idle)

    val state: StateFlow<FundingSummaryUiState?> =
        combine(sheet.flow.filterNotNull(), sheet.brands, start, sheet.entry) { flow, brands, start, entry ->
            flow.toSummaryUiState(brands, start, FundingSessionFacts.of(entry).failure)
        }.stateIn(this, WhileOnScreen, null)

    fun onFees() {
        if (sheet.flow.value?.selectedQuote != null) sheet.push(FundingScreen.FEES)
    }

    fun onCountry() = sheet.push(FundingScreen.COUNTRY)

    fun onProviders() = sheet.push(FundingScreen.PROVIDERS)

    /** Hands the session to the provider whose quote is on screen. A provider is only chosen on its quote. */
    fun onStart() {
        val providerId = sheet.flow.value?.selectedProviderId ?: return
        if (start.value == FundingStartState.Starting || start.value == FundingStartState.Started) return

        start.value = FundingStartState.Starting
        launchUnit {
            start.value = service.start(sheet.intent, providerId)
                .logFailure("Funding provider selection failed")
                .fold(onSuccess = { FundingStartState.Started }, onFailure = { FundingStartState.StartFailed })
        }
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingSummaryViewModel
    }
}
