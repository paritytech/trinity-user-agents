package io.paritytech.polkadotapp.tools_hydration_sdk_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class StandaloneHydrationSwapOverridableData @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
) : HydrationSwapOverridableData {
    override suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber> {
        return chainStateRepository.currentRemoteBlockNumberFlow(chainId, sharedRequestsBuilder = null)
    }
}
