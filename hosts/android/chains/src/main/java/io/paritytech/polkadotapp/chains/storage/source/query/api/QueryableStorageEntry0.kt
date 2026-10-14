package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntry
import io.novasama.substrate_sdk_android.runtime.metadata.storageKey
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.WithRawValue
import io.paritytech.polkadotapp.chains.util.decode
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlin.reflect.KType

typealias QueryableStorageBinder0<V> = (dynamicInstance: Any) -> V

interface QueryableStorageEntry0<T : Any> {
    val meta: StorageEntry

    context(storage: StorageQueryContext)
    suspend fun query(): T?

    context(storage: StorageQueryContext)
    suspend fun queryRaw(): String?

    context(storage: StorageQueryContext)
    fun observe(): Flow<T?>

    context(storage: StorageQueryContext)
    fun observeWithRaw(): Flow<WithRawValue<T?>>

    fun storageKey(): String

    fun decode(scale: String?): T?
}

context(storage: StorageQueryContext)
fun <T : Any> QueryableStorageEntry0<T>.observeNonNull(): Flow<T> = observe().filterNotNull()

context(storage: StorageQueryContext)
suspend fun <T : Any> QueryableStorageEntry0<T>.queryNonNull(): T = requireNotNull(query())

fun <T : Any> QueryableStorageEntry0<T>.decodeNonNull(scale: String?): T = requireNotNull(decode(scale))

internal class RealQueryableStorageEntry0<T : Any>(
    private val storageEntry: StorageEntry,
    private val encoders: Entry0Encoders<T>,
    private val runtimeSnapshot: RuntimeSnapshot,
) : QueryableStorageEntry0<T> {
    override val meta: StorageEntry = storageEntry

    context(storage: StorageQueryContext)
    override suspend fun query(): T? {
        return with(storage) {
            storageEntry.query(binding = { decoded -> decoded?.let(encoders::decodeValue) })
        }
    }

    context(storage: StorageQueryContext)
    override fun observe(): Flow<T?> {
        return with(storage) {
            storageEntry.observe(binding = { decoded -> decoded?.let(encoders::decodeValue) })
        }
    }

    context(storage: StorageQueryContext)
    override fun observeWithRaw(): Flow<WithRawValue<T?>> {
        return with(storage) {
            storageEntry.observeWithRaw(binding = { decoded -> decoded?.let(encoders::decodeValue) })
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun queryRaw(): String? {
        return with(storage) {
            storageEntry.queryRaw()
        }
    }

    override fun storageKey(): String {
        return storageEntry.storageKey()
    }

    override fun decode(scale: String?): T? {
        return storageEntry.decode(runtimeSnapshot, scale)?.let(encoders::decodeValue)
    }
}

sealed interface Entry0Encoders<T> {
    fun decodeValue(instance: Any): T

    class Auto<T>(
        valueType: KType
    ) : Entry0Encoders<T> {
        private val valueCodec = ScaleTypeCodec<T>(valueType)

        override fun decodeValue(instance: Any): T {
            return valueCodec.decode(instance)
        }
    }

    class Manual<T>(
        private val binding: QueryableStorageBinder0<T>,
    ) : Entry0Encoders<T> {
        override fun decodeValue(instance: Any): T {
            return binding(instance)
        }
    }
}
