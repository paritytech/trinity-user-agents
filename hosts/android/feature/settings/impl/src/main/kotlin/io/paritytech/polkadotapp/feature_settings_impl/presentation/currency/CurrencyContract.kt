package io.paritytech.polkadotapp.feature_settings_impl.presentation.currency

import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models.CurrencyState
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models.CurrencyUiModel
import kotlinx.coroutines.flow.StateFlow

interface CurrencyContract {
    val state: StateFlow<CurrencyState>

    fun onCurrencySelected(currency: CurrencyUiModel)
    fun onBackClick()
}
