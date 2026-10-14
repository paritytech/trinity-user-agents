package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntry
import io.paritytech.polkadotapp.chains.storage.source.query.StorageKeyComponents
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import kotlin.reflect.KType

data class StorageKey4<I1, I2, I3, I4>(
    val first: I1,
    val second: I2,
    val third: I3,
    val fourth: I4
)

interface QueryableStorageEntry4<I1, I2, I3, I4, T> {
    context(storage: StorageQueryContext)
    suspend fun query(key1: I1, key2: I2, key3: I3, key4: I4): T?

    context(storage: StorageQueryContext)
    suspend fun entries(keys: List<StorageKey4<I1, I2, I3, I4>>): Map<StorageKey4<I1, I2, I3, I4>, T?>

    context(storage: StorageQueryContext)
    suspend fun findExistingKeys(keys: Collection<StorageKey4<I1, I2, I3, I4>>): Set<StorageKey4<I1, I2, I3, I4>>
}

class RealQueryableStorageEntry4<I1, I2, I3, I4, T>(
    private val storageEntry: StorageEntry,
    key1Type: KType,
    key2Type: KType,
    key3Type: KType,
    key4Type: KType,
    valueType: KType
) : QueryableStorageEntry4<I1, I2, I3, I4, T> {
    private val key1Codec = ScaleTypeCodec<I1>(key1Type)
    private val key2Codec = ScaleTypeCodec<I2>(key2Type)
    private val key3Codec = ScaleTypeCodec<I3>(key3Type)
    private val key4Codec = ScaleTypeCodec<I4>(key4Type)
    private val valueCodec = ScaleTypeCodec<T>(valueType)

    context(storage: StorageQueryContext)
    override suspend fun query(key1: I1, key2: I2, key3: I3, key4: I4): T? {
        return with(storage) {
            storageEntry.query(
                key1Codec.encode(key1),
                key2Codec.encode(key2),
                key3Codec.encode(key3),
                key4Codec.encode(key4),
                binding = { decoded -> decoded?.let { valueCodec.decode(it) } }
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun entries(keys: List<StorageKey4<I1, I2, I3, I4>>): Map<StorageKey4<I1, I2, I3, I4>, T?> {
        return with(storage) {
            storageEntry.entries(
                keysArguments = keys.encoded(),
                keyExtractor = { it.bindKeys() },
                binding = { decoded, _ -> decoded?.let { valueCodec.decode(it) } },
            )
        }
    }

    context(storage: StorageQueryContext)
    override suspend fun findExistingKeys(keys: Collection<StorageKey4<I1, I2, I3, I4>>): Set<StorageKey4<I1, I2, I3, I4>> {
        return with(storage) {
            storageEntry.findExistingKeys(
                keysArguments = keys.encoded(),
                keyExtractor = { it.bindKeys() }
            )
        }
    }

    private fun StorageKeyComponents.bindKeys(): StorageKey4<I1, I2, I3, I4> {
        val (key1, key2, key3, key4) = values

        return StorageKey4(
            key1Codec.decode(key1),
            key2Codec.decode(key2),
            key3Codec.decode(key3),
            key4Codec.decode(key4),
        )
    }

    private fun Collection<StorageKey4<I1, I2, I3, I4>>.encoded() = map {
        listOf(
            key1Codec.encode(it.first),
            key2Codec.encode(it.second),
            key3Codec.encode(it.third),
            key4Codec.encode(it.fourth)
        )
    }
}
