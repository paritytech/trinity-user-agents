package io.paritytech.polkadotapp.chains.storage.source

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SubstrateSubscriptionBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.requests.subscribe
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.child.childStateKey
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.common.data.substrate.Binder
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

internal abstract class BaseStorageSource(
    protected val chainRegistry: ChainRegistry,
    private val sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    private val coroutineDispatchers: CoroutineDispatchers,
) : StorageDataSource {
    protected abstract suspend fun queryChildState(
        storageKey: String,
        childKey: String,
        chainId: String,
    ): String?

    protected abstract suspend fun createQueryContext(
        chainId: String,
        at: BlockHash?,
        runtime: RuntimeSnapshot,
        subscriptionBuilder: SubstrateSubscriptionBuilder?,
    ): StorageQueryContext

    override suspend fun <T> queryChildState(
        chainId: String,
        storageKeyBuilder: (RuntimeSnapshot) -> StorageKey,
        childKeyBuilder: ChildKeyBuilder,
        binder: Binder<T>,
    ) = withContext(coroutineDispatchers.computation) {
        val runtime = chainRegistry.getRuntime(chainId)

        val storageKey = storageKeyBuilder(runtime)

        val childKey =
            childStateKey {
                childKeyBuilder(runtime)
            }

        val scaleResult = queryChildState(storageKey, childKey, chainId)

        binder(scaleResult, runtime)
    }

    override suspend fun <R> query(
        chainId: String,
        at: BlockHash?,
        query: suspend StorageQueryContext.() -> R,
    ): R = withContext(coroutineDispatchers.computation) {
        val runtime = chainRegistry.getRuntime(chainId)
        val context = createQueryContext(chainId, at, runtime, subscriptionBuilder = null)

        context.query()
    }

    override fun <R> subscribe(
        chainId: String,
        at: BlockHash?,
        subscribe: suspend StorageQueryContext.() -> Flow<R>,
    ): Flow<R> {
        return flow {
            val runtime = chainRegistry.getRuntime(chainId)
            val context = createQueryContext(chainId, at, runtime, subscriptionBuilder = null)

            emitAll(context.subscribe())
        }
            .flowOn(coroutineDispatchers.computation)
    }

    override suspend fun <R> subscribe(
        chainId: String,
        subscriptionBuilder: SubstrateSubscriptionBuilder,
        at: BlockHash?,
        subscribe: suspend StorageQueryContext.() -> Flow<R>,
    ): Flow<R> {
        val runtime = chainRegistry.getRuntime(chainId)
        val context = createQueryContext(chainId, at, runtime, subscriptionBuilder)

        return subscribe(context)
            .flowOn(coroutineDispatchers.computation)
    }

    override suspend fun <R> subscribeBatched(
        chainId: String,
        at: BlockHash?,
        subscribe: suspend StorageQueryContext.() -> Flow<R>,
    ): Flow<R> {
        val runtime = chainRegistry.getRuntime(chainId)
        val sharedSubscription = sharedRequestsBuilderFactory.create(chainId)
        val context = createQueryContext(chainId, at, runtime, sharedSubscription)

        val result = subscribe(context)

        sharedSubscription.subscribe(coroutineContext)

        return result
            .flowOn(coroutineDispatchers.computation)
    }
}
