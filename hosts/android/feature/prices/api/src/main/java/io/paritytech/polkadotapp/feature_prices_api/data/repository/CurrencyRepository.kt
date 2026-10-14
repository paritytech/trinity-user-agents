package io.paritytech.polkadotapp.feature_prices_api.data.repository

import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import kotlinx.coroutines.flow.Flow

interface CurrencyRepository {
    val usd: Currency

    fun getAllSupportedCurrencies(): List<Currency>

    fun selectedCurrencyFlow(): Flow<Currency>

    suspend fun getSelectedCurrency(): Currency
    suspend fun updateSelectedCurrency(currency: Currency)
}
