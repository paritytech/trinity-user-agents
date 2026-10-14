package io.paritytech.polkadotapp.chains.network.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.utils.getOrEmpty
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach

abstract class MultipleStorageKeyUpdater<S>(
    override val scope: Updater.Scope<S>,
    private val chainRegistry: ChainRegistry,
    private val storageCache: StorageCache
) : Updater<S> {
    /**
     * @return a storage keys to update. Empty in case updater does not want to update anything
     */
    context(withRuntime: WithRuntime)
    abstract suspend fun storageKeys(scopeValue: S, chain: Chain): List<String>

    override suspend fun listenForUpdates(
        scopeValue: S,
        context: Updater.Context
    ): Flow<Updater.SideEffect> {
        return chainRegistry.withRuntime(context.chain.id) {
            val storageKeys = runCatching { storageKeys(scopeValue, context.chain) }.getOrEmpty()
            if (storageKeys.isEmpty()) return emptyFlow()

            val flows = storageKeys.map { key ->
                context.storageSubscriptionBuilder.subscribe(key)
                    .onEach { storageCache.insert(it, context.chain.id) }
                    .noSideAffects()
            }

            flows.merge()
        }
    }
}
