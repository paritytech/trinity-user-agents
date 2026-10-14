package io.paritytech.polkadotapp.chains.network.updaters.system

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers

class UpdateSystemFactory(
    private val chainRegistry: ChainRegistry,
    private val dispatchers: CoroutineDispatchers,
    private val storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory
) {
    /**
     * Creates an update system that can operate on given [chainId] using [updaters]
     * Note that [updaters] will be deduplicated by [ConstantSingleChainUpdateSystem] so you can
     * easily specify same updater multiple times which might be convenient if you are aggregating
     * them from difference sources/features
     */
    fun createConstantSingleChain(updaters: List<Updater<*>>, chainId: ChainId): UpdateSystem {
        return ConstantSingleChainUpdateSystem(updaters, chainId, chainRegistry, dispatchers, storageSharedRequestsBuilderFactory)
    }

    fun createCompound(vararg nested: UpdateSystem): UpdateSystem {
        return MultiChainUpdateSystem(*nested)
    }
}
