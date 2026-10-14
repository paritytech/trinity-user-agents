package io.paritytech.polkadotapp.chains.network.updaters.system

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flowOf
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.shareIn

abstract class SingleChainUpdateSystem<A>(
    chainRegistry: ChainRegistry,
    storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    dispatchers: CoroutineDispatchers,
) : ChainUpdaterGroupUpdateSystem(chainRegistry, storageSharedRequestsBuilderFactory, dispatchers) {
    abstract fun chainWithUpdaters(): Flow<Pair<Chain, Collection<Updater<*>>>>

    private val updateFlow by lazy {
        chainWithUpdaters().flatMapLatest { (chain, chainUpdaters) ->
            runUpdaters(chain, chainUpdaters)
        }
            .shareIn(CoroutineScope(dispatchers.io), replay = 1, started = SharingStarted.WhileSubscribed())
    }

    override fun start(): Flow<Updater.SideEffect> = updateFlow
}

class ConstantSingleChainUpdateSystem(
    private val updaters: List<Updater<*>>,
    private val chainId: ChainId,
    private val chainRegistry: ChainRegistry,
    dispatchers: CoroutineDispatchers,
    storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
) : SingleChainUpdateSystem<Any?>(chainRegistry, storageSharedRequestsBuilderFactory, dispatchers) {
    override fun chainWithUpdaters(): Flow<Pair<Chain, Collection<Updater<*>>>> {
        return flowOf {
            val chain = chainRegistry.getChain(chainId)

            chain to updaters.distinct()
        }
    }
}
