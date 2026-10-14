package io.paritytech.polkadotapp.feature_prices_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Price
import io.paritytech.polkadotapp.feature_prices_api.domain.model.PriceLookup
import kotlinx.coroutines.flow.Flow

interface GetPriceUseCase {
    suspend fun getPrice(chainAsset: Chain.Asset): Price

    fun priceFlow(chainAsset: Chain.Asset): Flow<Price>

    suspend fun getAllPrices(): PriceLookup

    suspend fun getPrices(assets: List<Chain.Asset>): PriceLookup

    fun allPricesFlow(): Flow<PriceLookup>
}
