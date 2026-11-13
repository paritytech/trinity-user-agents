package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingProvidersUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toProvidersUiState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.ProductId
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel(assistedFactory = FundingProvidersViewModel.Factory::class)
class FundingProvidersViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
) : BaseViewModel() {
    val state: StateFlow<FundingProvidersUiState?> = combine(sheet.flow.filterNotNull(), sheet.brands) { flow, brands ->
        flow.toProvidersUiState(brands)
    }.stateIn(this, WhileOnScreen, null)

    fun onProviderChosen(providerId: ProductId) {
        sheet.update { it.withChosenProvider(providerId) }
        sheet.message(RCommon.string.funding_provider_changed)
        sheet.pop(FundingScreen.PROVIDERS)
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingProvidersViewModel
    }
}
