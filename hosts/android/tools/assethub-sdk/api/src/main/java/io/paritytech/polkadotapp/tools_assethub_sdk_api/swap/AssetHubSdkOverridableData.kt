package io.paritytech.polkadotapp.tools_assethub_sdk_api.swap

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import kotlinx.coroutines.flow.Flow

interface AssetHubSdkOverridableData {
    suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber>
}
