package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.feature_prices_api.data.repository.CurrencyRepository
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

interface CurrencyInteractor {
    fun selectedCurrencyFlow(): Flow<Currency>
    suspend fun getSupportedCurrencies(): List<Currency>
    suspend fun updateSelectedCurrency(currency: Currency)
}

class RealCurrencyInteractor @Inject constructor(
    private val currencyRepository: CurrencyRepository
) : CurrencyInteractor {
    override fun selectedCurrencyFlow(): Flow<Currency> =
        currencyRepository.selectedCurrencyFlow()

    override suspend fun getSupportedCurrencies(): List<Currency> =
        currencyRepository.getAllSupportedCurrencies()

    override suspend fun updateSelectedCurrency(currency: Currency) =
        currencyRepository.updateSelectedCurrency(currency)
}
