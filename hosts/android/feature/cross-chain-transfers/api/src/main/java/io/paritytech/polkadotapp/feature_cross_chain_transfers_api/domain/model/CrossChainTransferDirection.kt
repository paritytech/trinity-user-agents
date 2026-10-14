package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.utils.graph.Edge
import io.paritytech.polkadotapp.common.utils.graph.SimpleEdge

typealias CrossChainTransferDirection = Edge<ChainWithAsset>

fun CrossChainTransferDirection(
    originChain: Chain,
    originAsset: Chain.Asset,
    destinationChain: Chain,
    destinationAsset: Chain.Asset
): CrossChainTransferDirection {
    return SimpleEdge(
        from = ChainWithAsset(originChain, originAsset),
        to = ChainWithAsset(destinationChain, destinationAsset)
    )
}
