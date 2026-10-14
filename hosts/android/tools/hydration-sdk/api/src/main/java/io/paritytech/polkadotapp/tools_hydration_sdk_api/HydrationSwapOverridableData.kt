package io.paritytech.polkadotapp.tools_hydration_sdk_api

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import kotlinx.coroutines.flow.Flow

interface HydrationSwapOverridableData {
    suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber>
}
