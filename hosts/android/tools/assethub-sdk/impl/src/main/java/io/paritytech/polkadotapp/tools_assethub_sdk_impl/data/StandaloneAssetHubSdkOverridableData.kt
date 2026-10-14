package io.paritytech.polkadotapp.tools_assethub_sdk_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSdkOverridableData
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class StandaloneAssetHubSdkOverridableData @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
) : AssetHubSdkOverridableData {
    override suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber> {
        return chainStateRepository.currentRemoteBlockNumberFlow(chainId, sharedRequestsBuilder = null)
    }
}
