package io.paritytech.polkadotapp.feature_prices_impl.domain

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalCache
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.memory.useCache
import io.paritytech.polkadotapp.feature_prices_api.domain.GetCachedPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.GetPriceUseCase
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Price
import javax.inject.Inject

internal class RealGetCachedPriceUseCase @Inject constructor(
    private val getPriceUseCase: GetPriceUseCase,
    private val computationalCache: ComputationalCache
) : GetCachedPriceUseCase {
    context(scope: ComputationalScope)
    override suspend fun getPrice(chainAsset: Chain.Asset): Price {
        val key = "PRICE_FOR_ASSET:${chainAsset.id}"

        return computationalCache.useCache(key) {
            getPriceUseCase.getPrice(chainAsset)
        }
    }
}
