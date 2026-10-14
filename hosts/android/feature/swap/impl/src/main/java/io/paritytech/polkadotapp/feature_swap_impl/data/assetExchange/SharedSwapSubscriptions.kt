package io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import kotlinx.coroutines.flow.Flow

interface SharedSwapSubscriptions {
    suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber>
}
