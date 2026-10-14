package io.paritytech.polkadotapp.chains.storage.source.multi

import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntry
import io.paritytech.polkadotapp.chains.storage.source.query.DynamicInstanceBinder
import io.paritytech.polkadotapp.chains.storage.source.query.StorageKeyComponents
import io.paritytech.polkadotapp.chains.storage.source.query.wrapSingleArgumentKeys

interface MultiQueryBuilder {
    interface Descriptor<K, V> {
        fun parseKey(key: String): K

        fun parseValue(value: String?): V
    }

    interface Result {
        operator fun <K, V> get(descriptor: Descriptor<K, V>): Map<K, V>
    }

    fun <V> StorageEntry.queryKey(
        vararg args: Any?,
        binding: DynamicInstanceBinder<V>,
    ): Descriptor<StorageKeyComponents, V>

    fun <K, V> StorageEntry.queryKeys(
        keysArgs: List<List<Any?>>,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinder<V>,
    ): Descriptor<K, V>

    fun <K, V> StorageEntry.querySingleArgKeys(
        keysArgs: Iterable<Any?>,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinder<V>,
    ): Descriptor<K, V> = queryKeys(keysArgs.wrapSingleArgumentKeys(), keyExtractor, binding)
}

fun <V> MultiQueryBuilder.Result.singleValueOf(descriptor: MultiQueryBuilder.Descriptor<*, V>): V = get(descriptor).values.first()
