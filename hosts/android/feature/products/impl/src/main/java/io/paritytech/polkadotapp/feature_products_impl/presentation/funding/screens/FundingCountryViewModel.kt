package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCountries
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCountryUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toCountryUiState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn

/** Every country, searched by name or currency, with what the core says of each on the rail and asset picked. */
@HiltViewModel(assistedFactory = FundingCountryViewModel.Factory::class)
class FundingCountryViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
) : BaseViewModel() {
    private val query = MutableStateFlow("")
    private val countries by lazy { FundingCountries.all() }

    val state: StateFlow<FundingCountryUiState?> = combine(sheet.flow.filterNotNull(), query) { flow, query ->
        val detected = flow.options?.detectedCountry?.let(FundingCountries::country)
        flow.toCountryUiState(countries, detected, query)
    }.stateIn(this, WhileOnScreen, null)

    fun onQueryChanged(text: String) {
        query.value = text
    }

    fun onCountryChosen(code: String) {
        val chosen = countries.firstOrNull { it.code == code } ?: return
        sheet.update { it.withCountry(chosen) }
        sheet.pop(FundingScreen.COUNTRY)
        sheet.requote()
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingCountryViewModel
    }
}
