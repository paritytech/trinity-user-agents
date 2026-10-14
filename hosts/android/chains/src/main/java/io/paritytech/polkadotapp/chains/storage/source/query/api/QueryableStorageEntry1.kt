package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntry
import io.novasama.substrate_sdk_android.runtime.metadata.storageKey
import io.novasama.substrate_sdk_android.runtime.metadata.storageOrNull
import io.paritytech.polkadotapp.chains.storage.source.query.StorageKeyComponents
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.WithRawValue
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.decode
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlin.reflect.KType

typealias QueryableStorageBinder1<K, V> = (dynamicInstance: Any, key: K) -> V

interface QueryableStorageEntry1<I, T> {
    context(storage: StorageQueryContext)
    suspend fun keys(): List<I>

    context(storage: StorageQueryContext)
    suspend fun entries(): Map<I, T>

    context(storage: StorageQueryContext)
    suspend fun entries(keys: Collection<I>): Map<I, T>

    context(storage: StorageQueryContext)
    suspend fun query(argument: I): T?

    context(storage: StorageQueryContext)
    suspend fun queryRaw(argument: I): String?

    context(storage: StorageQueryContext)
    fun observe(argument: I): Flow<T?>

    context(storage: StorageQueryContext)
    suspend fun observe(arguments: List<I>): Flow<Map<I, T?>>

    context(storage: StorageQueryContext)
    fun observeWithRaw(argument: I): Flow<WithRawValue<T?>>

    fun storageKey(argument: I): String

    fun decode(scale: String?, argument: I): T?
}

context(storage: StorageQueryContext)
fun <I, T : Any> QueryableStorageEntry1<I, T>.observeNonNull(argument: I): Flow<T> = observe(argument).filterNotNull()

context(storage: StorageQueryContext)
suspend fun <I, T : Any> QueryableStorageEntry1<I, T>.queryNonNull(argument: I): T = requireNotNull(query(argument))

context(withRuntime: WithRuntime)
fun <I, T> QueryableModule.storage1OrNull(
    name: String,
    binding: QueryableStorageBinder1<I, T>,
    toKeyBinding: QueryableStorageToKeyBinder<I>? = null,
    fromKeyBinding: QueryableStorageFromKeyBinder<I>? = null,
): QueryableStorageEntry1<I, T>? {
    return module.storageOrNull(name)?.let {
        val encoders = Entry1Encoders.Manual(binding, toKeyBinding, fromKeyBinding)
        RealQueryableStorageEntry1(withRuntime.runtime, it, encoders)
    }
}

fun <I, T : Any> QueryableStorageEntry1<I, T>.decodeNonNull(scale: String?, argument: I): T = requireNotNull(decode(scale, argument))

internal class RealQueryableStorageEntry1<I, T>(
    private val runtimeSnapshot: RuntimeSnapshot,
    private val storageEntry: StorageEntry,
    private val encoders: Entry1Encoders<I, T>
) : QueryableStorageEntry1<I, T> {
    context(storage: StorageQueryContext)
    override suspend fun query(argument: I): T? {
        return with(storage) {
            storageEntry.query(encoders.encodeKey(argument), binding = { decoded -> decoded?.let { encoders.decodeValue(it, argument) } })
        }
    }

    context(storage: StorageQueryContext)
    override fun observe(argument: I): Flow<T?> {
        return with(storage) {
            storageEntry.observe(encoders.encodeKey(argument), binding = { decoded -> decoded?.let { encoders.decodeValue(it, argument) } })
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun observe(arguments: List<I>): Flow<Map<I, T?>> {
        return with(storage) {
            storageEntry.observe(
                keysArguments = arguments.map { listOf(encoders.encodeKey(it)) },
                keyExtractor = ::bindKey,
                binding = { decoded, key -> decoded?.let { encoders.decodeValue(it, key) } as T }
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun queryRaw(argument: I): String? {
        return with(storage) {
            storageEntry.queryRaw(encoders.encodeKey(argument))
        }
    }

    override fun storageKey(argument: I): String {
        return storageEntry.storageKey(runtimeSnapshot, encoders.encodeKey(argument))
    }

    context(storage: StorageQueryContext)
    override fun observeWithRaw(argument: I): Flow<WithRawValue<T?>> {
        return with(storage) {
            storageEntry.observeWithRaw(encoders.encodeKey(argument), binding = { decoded -> decoded?.let { encoders.decodeValue(it, argument) } })
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun keys(): List<I> {
        return with(storage) {
            storageEntry.keys().map(::bindKey)
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun entries(): Map<I, T> {
        return with(storage) {
            storageEntry.entries(
                keyExtractor = ::bindKey,
                binding = { decoded, key -> decoded?.let { encoders.decodeValue(it, key) } as T }
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun entries(keys: Collection<I>): Map<I, T> {
        return with(storage) {
            storageEntry.entries(
                keysArguments = keys.map { listOf(encoders.encodeKey(it)) },
                keyExtractor = ::bindKey,
                binding = { decoded, key -> decoded?.let { encoders.decodeValue(it, key) } as T }
            )
        }
    }

    override fun decode(scale: String?, argument: I): T? {
        return storageEntry.decode(runtimeSnapshot, scale)?.let { encoders.decodeValue(it, argument) }
    }

    private fun bindKey(storageKeyComponents: StorageKeyComponents): I {
        val firstComponent = storageKeyComponents.component1<Any?>()
        return encoders.decodeKey(firstComponent)
    }
}

sealed interface Entry1Encoders<I, T> {
    fun decodeValue(instance: Any, key: I): T

    fun decodeKey(instance: Any?): I

    fun encodeKey(key: I): Any?

    class Auto<I, T>(
        keyType: KType,
        valueType: KType
    ) : Entry1Encoders<I, T> {
        private val keyCodec = ScaleTypeCodec<I>(keyType)
        private val valueCodec = ScaleTypeCodec<T>(valueType)

        override fun decodeValue(instance: Any, key: I): T {
            return valueCodec.decode(instance)
        }

        override fun decodeKey(instance: Any?): I {
            return keyCodec.decode(instance)
        }

        override fun encodeKey(key: I): Any? {
            return keyCodec.encode(key)
        }
    }

    class Manual<I, T>(
        private val binding: QueryableStorageBinder1<I, T>,
        toKeyBinding: QueryableStorageToKeyBinder<I>?,
        fromKeyBinding: QueryableStorageFromKeyBinder<I>?,
    ) : Entry1Encoders<I, T> {
        @Suppress("UNCHECKED_CAST")
        private val toKeyBindingOrDefault = toKeyBinding ?: { it as I }
        private val fromKeyBindingOrDefault = fromKeyBinding ?: { it }

        override fun decodeValue(instance: Any, key: I): T {
            return binding(instance, key)
        }

        override fun decodeKey(instance: Any?): I {
            return toKeyBindingOrDefault(instance)
        }

        override fun encodeKey(key: I): Any? {
            return fromKeyBindingOrDefault(key)
        }
    }
}
