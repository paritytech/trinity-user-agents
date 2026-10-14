package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

fun interface BlockProductionAnchorSource {
    suspend fun fetch(chainId: ChainId, expectedBlocks: Int): Result<BlockProductionAnchor>
}
