package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntry
import io.paritytech.polkadotapp.chains.storage.source.query.StorageKeyComponents
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.WithRawValue
import kotlinx.coroutines.flow.Flow
import kotlin.reflect.KType

interface QueryableStorageEntry3<I1, I2, I3, T> {
    context(storage: StorageQueryContext)
    suspend fun query(key1: I1, key2: I2, key3: I3): T?

    context(storage: StorageQueryContext)
    fun observeWithRaw(key1: I1, key2: I2, key3: I3): Flow<WithRawValue<T?>>

    context(storage: StorageQueryContext)
    suspend fun entries(key1: I1, key2: I2): Map<Triple<I1, I2, I3>, T?>

    context(storage: StorageQueryContext)
    suspend fun entries(keys: List<Triple<I1, I2, I3>>): Map<Triple<I1, I2, I3>, T?>

    context(storage: StorageQueryContext)
    suspend fun findExistingKeys(keys: Collection<Triple<I1, I2, I3>>): Set<Triple<I1, I2, I3>>

    context(storage: StorageQueryContext)
    suspend fun keyExists(argument1: I1, argument2: I2, argument3: I3): Boolean
}

context(storage: StorageQueryContext)
suspend fun <I1, I2, I3, T : Any> QueryableStorageEntry3<I1, I2, I3, T>.queryNonNull(key1: I1, key2: I2, key3: I3): T = with(storage) { requireNotNull(query(key1, key2, key3)) }

class RealQueryableStorageEntry3<I1, I2, I3, T>(
    private val storageEntry: StorageEntry,
    key1Type: KType,
    key2Type: KType,
    key3Type: KType,
    valueType: KType
) : QueryableStorageEntry3<I1, I2, I3, T> {
    private val key1Codec = ScaleTypeCodec<I1>(key1Type)
    private val key2Codec = ScaleTypeCodec<I2>(key2Type)
    private val key3Codec = ScaleTypeCodec<I3>(key3Type)
    private val valueCodec = ScaleTypeCodec<T>(valueType)

    context(storage: StorageQueryContext)
    override suspend fun query(key1: I1, key2: I2, key3: I3): T? {
        return with(storage) {
            storageEntry.query(
                key1Codec.encode(key1),
                key2Codec.encode(key2),
                key3Codec.encode(key3),
                binding = { decoded -> decoded?.let { valueCodec.decode(it) } }
            )
        }
    }

    context(storage: StorageQueryContext)
    override fun observeWithRaw(key1: I1, key2: I2, key3: I3): Flow<WithRawValue<T?>> {
        return with(storage) {
            storageEntry.observeWithRaw(
                key1Codec.encode(key1),
                key2Codec.encode(key2),
                key3Codec.encode(key3),
                binding = { decoded -> decoded?.let { valueCodec.decode(it) } }
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun entries(key1: I1, key2: I2): Map<Triple<I1, I2, I3>, T?> {
        return with(storage) {
            storageEntry.entries(
                key1Codec.encode(key1),
                key2Codec.encode(key2),
                keyExtractor = { it.bindKeys() },
                binding = { decoded, _ -> decoded?.let { valueCodec.decode(it) } },
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun entries(keys: List<Triple<I1, I2, I3>>): Map<Triple<I1, I2, I3>, T?> {
        return with(storage) {
            storageEntry.entries(
                keysArguments = keys.encoded(),
                keyExtractor = { it.bindKeys() },
                binding = { decoded, _ -> decoded?.let { valueCodec.decode(it) } },
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun findExistingKeys(keys: Collection<Triple<I1, I2, I3>>): Set<Triple<I1, I2, I3>> {
        return with(storage) {
            storageEntry.findExistingKeys(
                keysArguments = keys.encoded(),
                keyExtractor = { it.bindKeys() }
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun keyExists(argument1: I1, argument2: I2, argument3: I3): Boolean {
        val key = Triple(argument1, argument2, argument3)
        val existingKeys = with(storage) { findExistingKeys(listOf(key)) }

        return existingKeys.contains(key)
    }

    private fun StorageKeyComponents.bindKeys(): Triple<I1, I2, I3> {
        val (key1, key2, key3) = values

        return Triple(
            key1Codec.decode(key1),
            key2Codec.decode(key2),
            key3Codec.decode(key3),
        )
    }

    private fun Collection<Triple<I1, I2, I3>>.encoded() = map {
        listOf(
            key1Codec.encode(it.first),
            key2Codec.encode(it.second),
            key3Codec.encode(it.third)
        )
    }
}
