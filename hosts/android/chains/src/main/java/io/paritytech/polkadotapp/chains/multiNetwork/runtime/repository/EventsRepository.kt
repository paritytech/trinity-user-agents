package io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash

interface EventsRepository {
    /**
     * @return events in block corresponding to [blockHash] or in current block, if [blockHash] is null
     * Unparsed events are not included
     */
    suspend fun getEventsInBlock(
        chainId: ChainId,
        blockHash: BlockHash? = null,
    ): BlockEvents
}

class RemoteEventsRepository(
    private val chainRegistry: ChainRegistry,
    private val delegateFactory: ChainEventsRepositoryFactory
) : EventsRepository {
    override suspend fun getEventsInBlock(
        chainId: ChainId,
        blockHash: BlockHash?,
    ): BlockEvents {
        return createDelegate(chainId).getEventsInBlock(blockHash)
    }

    private suspend fun createDelegate(chainId: ChainId): ChainEventsRepository {
        return delegateFactory.create(chainId)
    }
}
