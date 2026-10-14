package io.paritytech.polkadotapp.chains.storage.source

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SubstrateSubscriptionBuilder
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.common.data.substrate.Binder
import io.paritytech.polkadotapp.common.utils.wrapIntoResult
import kotlinx.coroutines.flow.Flow
import java.io.OutputStream

typealias StorageKey = String
typealias ChildKeyBuilder = suspend OutputStream.(RuntimeSnapshot) -> Unit

interface StorageDataSource {
    suspend fun <T> queryChildState(
        chainId: String,
        storageKeyBuilder: (RuntimeSnapshot) -> StorageKey,
        childKeyBuilder: ChildKeyBuilder,
        binder: Binder<T>,
    ): T

    suspend fun <R> query(
        chainId: String,
        at: BlockHash? = null,
        query: suspend StorageQueryContext.() -> R,
    ): R

    fun <R> subscribe(
        chainId: String,
        at: BlockHash? = null,
        subscribe: suspend StorageQueryContext.() -> Flow<R>,
    ): Flow<R>

    // Note: we cannot combine this method with [subscribe] without no subscriptionBuilder
    // Since when [subscriptionBuilder] is present it has to be suspend
    // In case you want to dynamically switch between them, consider using [subscribeWithOptionalSharing]
    suspend fun <R> subscribe(
        chainId: String,
        subscriptionBuilder: SubstrateSubscriptionBuilder,
        at: BlockHash? = null,
        subscribe: suspend StorageQueryContext.() -> Flow<R>,
    ): Flow<R>

    /**
     * Aggregates all requests called via [subscribe] block and executes them as a single batch request, if possible
     * Lifecycle of subscription is bound to parent coroutine
     */
    suspend fun <R> subscribeBatched(
        chainId: String,
        at: BlockHash? = null,
        subscribe: suspend StorageQueryContext.() -> Flow<R>,
    ): Flow<R>
}

suspend fun <R> StorageDataSource.subscribeWithOptionalSharing(
    chainId: String,
    subscriptionBuilder: SubstrateSubscriptionBuilder? = null,
    at: BlockHash? = null,
    subscribe: suspend StorageQueryContext.() -> Flow<R>,
): Flow<R> {
    return if (subscriptionBuilder == null) {
        subscribe(chainId, at, subscribe)
    } else {
        subscribe(chainId, subscriptionBuilder, at, subscribe)
    }
}

suspend fun <R> StorageDataSource.queryCatching(
    chainId: String,
    at: BlockHash? = null,
    query: suspend StorageQueryContext.() -> R,
): Result<R> = runCatching { query(chainId, at, query) }

fun <R> StorageDataSource.subscribeCatching(
    chainId: String,
    at: BlockHash? = null,
    subscribe: suspend StorageQueryContext.() -> Flow<R>,
): Flow<Result<R>> {
    return subscribe(chainId, at, subscribe).wrapIntoResult()
}
