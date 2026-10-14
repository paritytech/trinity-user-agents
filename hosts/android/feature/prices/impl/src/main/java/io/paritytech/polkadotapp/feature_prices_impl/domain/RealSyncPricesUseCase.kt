package io.paritytech.polkadotapp.feature_prices_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.allAssets
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_prices_api.data.repository.CurrencyRepository
import io.paritytech.polkadotapp.feature_prices_api.data.repository.PriceRepository
import io.paritytech.polkadotapp.feature_prices_api.domain.SyncPricesUseCase
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class RealSyncPricesUseCase @Inject constructor(
    private val currencyRepository: CurrencyRepository,
    private val priceRepository: PriceRepository,
    private val chainRegistry: ChainRegistry,
    private val coroutineDispatchers: CoroutineDispatchers,
) : SyncPricesUseCase {
    override suspend fun syncPrices(): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            val currency = currencyRepository.getSelectedCurrency()
            val allAssets = chainRegistry.allAssets()

            priceRepository.syncPrices(currency, allAssets)
        }
    }
}
