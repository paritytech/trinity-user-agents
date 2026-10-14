package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.feature_prices_api.domain.SyncPricesUseCase
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import javax.inject.Inject

class SyncPriceCurrencyChange @Inject constructor(
    private val syncPricesUseCase: SyncPricesUseCase,
    private val currencyInteractor: CurrencyInteractor
) {
    suspend fun startObserving() {
        currencyInteractor.selectedCurrencyFlow()
            .distinctUntilChanged()
            .drop(1)
            .collect {
                runCatching {
                    syncPricesUseCase.syncPrices()
                }
            }
    }
}
