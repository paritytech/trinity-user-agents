package io.paritytech.polkadotapp.chains.storage.cache

import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.storage.source.query.WithRawValue
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

interface StorageCachingContext {
    val storageCache: StorageCache
}

context(storageCachingContext: StorageCachingContext)
fun <T> Flow<WithRawValue<T>>.cacheValues(): Flow<T> {
    return map {
        storageCachingContext.storageCache.insert(it.raw, it.chainId)

        it.value
    }
}

fun StorageCachingContext(storageCache: StorageCache): StorageCachingContext {
    return InlineStorageCachingContext(storageCache)
}

@JvmInline
private value class InlineStorageCachingContext(override val storageCache: StorageCache) : StorageCachingContext
