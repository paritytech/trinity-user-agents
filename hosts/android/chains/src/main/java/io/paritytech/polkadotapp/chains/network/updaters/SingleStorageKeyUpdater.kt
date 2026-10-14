package io.paritytech.polkadotapp.chains.network.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.storage.StorageChange
import io.paritytech.polkadotapp.chains.storage.StorageEntry
import io.paritytech.polkadotapp.chains.util.WithRuntime
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.onEach

suspend fun StorageCache.insert(
    storageChange: StorageChange,
    chainId: String,
) {
    val storageEntry = StorageEntry(
        storageKey = storageChange.key,
        content = storageChange.value,
    )

    insert(storageEntry, chainId)
}

abstract class SingleStorageKeyUpdater<S>(
    override val scope: Updater.Scope<S>,
    private val chainRegistry: ChainRegistry,
    private val storageCache: StorageCache
) : Updater<S> {
    /**
     * @return a storage key to update. null in case updater does not want to update anything
     */
    context (withRuntime: WithRuntime)
    abstract suspend fun storageKey(scopeValue: S, chain: Chain): String?

    override suspend fun listenForUpdates(
        scopeValue: S,
        context: Updater.Context
    ): Flow<Updater.SideEffect> {
        return chainRegistry.withRuntime(context.chain.id) {
            val storageKey = runCatching { storageKey(scopeValue, context.chain) }.getOrNull() ?: return emptyFlow()

            context.storageSubscriptionBuilder.subscribe(storageKey)
                .onEach { storageCache.insert(it, context.chain.id) }
                .noSideAffects()
        }
    }
}
