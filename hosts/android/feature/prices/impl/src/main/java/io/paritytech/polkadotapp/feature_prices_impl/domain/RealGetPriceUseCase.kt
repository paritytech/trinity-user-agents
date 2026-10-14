package io.paritytech.polkadotapp.feature_prices_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_prices_api.data.repository.CurrencyRepository
import io.paritytech.polkadotapp.feature_prices_api.data.repository.PriceRepository
import io.paritytech.polkadotapp.feature_prices_api.domain.GetPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Price
import io.paritytech.polkadotapp.feature_prices_api.domain.model.PriceLookup
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flatMapLatest
import javax.inject.Inject

internal class RealGetPriceUseCase @Inject constructor(
    private val priceRepository: PriceRepository,
    private val currencyRepository: CurrencyRepository,
) : GetPriceUseCase {
    override suspend fun getPrice(chainAsset: Chain.Asset): Price {
        val currency = currencyRepository.getSelectedCurrency()
        return priceRepository.getPrice(chainAsset, currency)
    }

    override fun priceFlow(chainAsset: Chain.Asset): Flow<Price> {
        return currencyRepository.selectedCurrencyFlow().flatMapLatest { currency ->
            priceRepository.priceFlow(chainAsset, currency)
        }
    }

    override suspend fun getAllPrices(): PriceLookup {
        val currency = currencyRepository.getSelectedCurrency()
        return priceRepository.getAllPrices(currency)
    }

    override suspend fun getPrices(assets: List<Chain.Asset>): PriceLookup {
        val currency = currencyRepository.getSelectedCurrency()
        return priceRepository.getPrices(currency, assets)
    }

    override fun allPricesFlow(): Flow<PriceLookup> {
        return currencyRepository.selectedCurrencyFlow().flatMapLatest { currency ->
            priceRepository.allPricesFlow(currency)
        }
    }
}
