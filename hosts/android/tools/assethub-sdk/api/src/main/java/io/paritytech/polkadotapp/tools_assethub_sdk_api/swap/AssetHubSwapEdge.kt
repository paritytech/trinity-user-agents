package io.paritytech.polkadotapp.tools_assethub_sdk_api.swap

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.utils.graph.Edge
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapDirection

interface AssetHubSwapEdge : Edge<FullChainAssetId> {
    val fromAsset: Chain.Asset

    val toAsset: Chain.Asset

    override val from: FullChainAssetId
        get() = fromAsset.fullId

    override val to: FullChainAssetId
        get() = toAsset.fullId

    suspend fun quote(
        amount: Balance,
        direction: SwapDirection
    ): Balance
}
