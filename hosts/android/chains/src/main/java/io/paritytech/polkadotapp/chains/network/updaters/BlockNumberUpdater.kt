package io.paritytech.polkadotapp.chains.network.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.updaters.scope.GlobalUpdaterScope
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.storage.typed.number
import io.paritytech.polkadotapp.chains.storage.typed.system
import io.paritytech.polkadotapp.chains.util.WithRuntime

class BlockNumberUpdater(
    chainRegistry: ChainRegistry,
    storageCache: StorageCache
) : SingleStorageKeyUpdater<Unit>(GlobalUpdaterScope, chainRegistry, storageCache) {
    context(withRuntime: WithRuntime)
    override suspend fun storageKey(scopeValue: Unit, chain: Chain): String {
        return withRuntime.runtime.metadata.system.number.storageKey()
    }
}
