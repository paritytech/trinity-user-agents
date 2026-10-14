package io.paritytech.polkadotapp.feature_prices_api.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Price

interface GetCachedPriceUseCase {
    context(scope: ComputationalScope)
    suspend fun getPrice(chainAsset: Chain.Asset): Price
}
