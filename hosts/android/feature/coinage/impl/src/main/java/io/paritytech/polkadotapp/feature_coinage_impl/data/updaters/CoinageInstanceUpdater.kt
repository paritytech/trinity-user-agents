package io.paritytech.polkadotapp.feature_coinage_impl.data.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.updaters.SingleStorageKeyUpdater
import io.paritytech.polkadotapp.chains.network.updaters.scope.GlobalUpdaterScope
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.coinage
import io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain.instances
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import javax.inject.Inject

class CoinageInstanceUpdater @Inject constructor(
    chainRegistry: ChainRegistry,
    storageCache: StorageCache,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) : SingleStorageKeyUpdater<Unit>(
    scope = GlobalUpdaterScope,
    chainRegistry = chainRegistry,
    storageCache = storageCache,
) {
    context(withRuntime: WithRuntime)
    override suspend fun storageKey(scopeValue: Unit, chain: Chain): String? {
        val instanceId = coinageInstanceIdProvider.instanceId().getOrNull() ?: return null

        return withRuntime.runtime.metadata.coinage.instances.storageKey(instanceId.toLong().toBigInteger())
    }
}
