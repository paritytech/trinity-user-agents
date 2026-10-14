package io.paritytech.polkadotapp.tools_hydration_sdk_api.swap

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.graph.WeightedEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection

interface HydrationSwapEdge : WeightedEdge<FullChainAssetId> {
    suspend fun debugLabel(): String

    suspend fun quote(
        amount: Balance,
        direction: SwapDirection
    ): Balance

    fun routerPoolArgument(): DictEnum.Entry<*>
}
