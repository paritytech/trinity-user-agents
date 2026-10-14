package io.paritytech.polkadotapp.chains.storage.source

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SubstrateSubscriptionBuilder
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.storage.source.query.LocalStorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers

internal class LocalStorageSource(
    chainRegistry: ChainRegistry,
    sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    coroutineDispatchers: CoroutineDispatchers,
    private val storageCache: StorageCache,
) : BaseStorageSource(chainRegistry, sharedRequestsBuilderFactory, coroutineDispatchers) {
    override suspend fun queryChildState(
        storageKey: String,
        childKey: String,
        chainId: String,
    ): String? {
        throw NotImplementedError("Child state queries are not yet supported in local storage")
    }

    override suspend fun createQueryContext(
        chainId: String,
        at: BlockHash?,
        runtime: RuntimeSnapshot,
        subscriptionBuilder: SubstrateSubscriptionBuilder?,
    ): StorageQueryContext {
        return LocalStorageQueryContext(storageCache, chainId, at, runtime)
    }
}
