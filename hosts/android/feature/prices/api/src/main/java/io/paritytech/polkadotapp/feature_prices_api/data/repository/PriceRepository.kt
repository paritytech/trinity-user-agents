package io.paritytech.polkadotapp.feature_prices_api.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_prices_api.domain.model.HistoricalPrice
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Price
import io.paritytech.polkadotapp.feature_prices_api.domain.model.PriceLookup
import kotlinx.coroutines.flow.Flow

interface PriceRepository {
    suspend fun syncPrices(currency: Currency, allAssets: List<Chain.Asset>): Result<Unit>

    suspend fun getPrice(chainAsset: Chain.Asset, currency: Currency): Price

    fun priceFlow(chainAsset: Chain.Asset, currency: Currency): Flow<Price>

    suspend fun getAllPrices(currency: Currency): PriceLookup

    suspend fun getPrices(currency: Currency, assets: List<Chain.Asset>): PriceLookup

    fun allPricesFlow(currency: Currency): Flow<PriceLookup>

    suspend fun fetchAllHistoricalRates(chainAsset: Chain.Asset, currency: Currency): Result<List<HistoricalPrice>>
}
