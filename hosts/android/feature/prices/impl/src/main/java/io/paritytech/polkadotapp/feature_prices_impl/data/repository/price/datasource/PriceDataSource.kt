package io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model.HistoricalPriceRemote
import io.paritytech.polkadotapp.feature_prices_impl.data.repository.price.datasource.model.PriceRemote

internal interface PriceDataSource {
    suspend fun fetchPrices(assets: List<Chain.Asset>, currency: Currency): List<PriceRemote>

    suspend fun fetchAllHistoricalPrices(
        asset: Chain.Asset,
        currency: Currency
    ): List<HistoricalPriceRemote>
}
