package io.paritytech.polkadotapp.feature_prices_impl.data.repository

import io.paritytech.polkadotapp.feature_prices_api.data.repository.CurrencyRepository
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_prices_impl.data.storage.CurrencyStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

internal class RealCurrencyRepository @Inject constructor(
    private val currencyStorage: CurrencyStorage
) : CurrencyRepository {
    override val usd: Currency = Currency.USD

    override fun getAllSupportedCurrencies(): List<Currency> {
        return Currency.entries
    }

    override fun selectedCurrencyFlow(): Flow<Currency> {
        return currencyStorage.valueFlow().map { it ?: Currency.USD }
    }

    override suspend fun getSelectedCurrency(): Currency {
        return currencyStorage.getValue() ?: Currency.USD
    }

    override suspend fun updateSelectedCurrency(currency: Currency) {
        currencyStorage.saveValue(currency)
    }
}
