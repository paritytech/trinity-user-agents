package io.paritytech.polkadotapp.chains.network.updaters.system

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.requests.subscribe
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.mergeIfMultiple
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import timber.log.Timber
import kotlin.coroutines.coroutineContext

abstract class ChainUpdaterGroupUpdateSystem(
    private val chainRegistry: ChainRegistry,
    private val storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    private val dispatchers: CoroutineDispatchers,
) : UpdateSystem {
    protected suspend fun runUpdaters(chain: Chain, updaters: Collection<Updater<*>>): Flow<Updater.SideEffect> {
        val selfName = this@ChainUpdaterGroupUpdateSystem::class.java.simpleName

        val scopeFlows = updaters.groupBy(Updater<*>::scope).map { (scope, scopeUpdaters) ->
            scope.invalidationFlow(chain).flatMapLatest { scopeValue ->
                val subscriptionBuilder = storageSharedRequestsBuilderFactory.create(chain.id)

                val context = Updater.Context(subscriptionBuilder, chain)

                val updatersFlow = scopeUpdaters
                    .map { updater ->
                        @Suppress("UNCHECKED_CAST")
                        (updater as Updater<Any?>).listenForUpdates(scopeValue, context)
                            .catch { Timber.e("Failed to start $selfName for ${chain.name}: ${it.message}") }
                            .flowOn(dispatchers.io)
                    }

                if (updatersFlow.isNotEmpty()) {
                    subscriptionBuilder.subscribe(coroutineContext)

                    updatersFlow.mergeIfMultiple()
                } else {
                    emptyFlow()
                }
            }
        }

        return scopeFlows.mergeIfMultiple()
    }
}
