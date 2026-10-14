package io.paritytech.polkadotapp.chains.storage.source.query

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.primitives.u16
import io.novasama.substrate_sdk_android.runtime.definitions.types.toByteArray
import io.novasama.substrate_sdk_android.runtime.metadata.StorageEntryModifier
import io.novasama.substrate_sdk_android.runtime.metadata.module.Constant
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntry
import io.novasama.substrate_sdk_android.runtime.metadata.module.StorageEntryType
import io.novasama.substrate_sdk_android.runtime.metadata.splitKey
import io.novasama.substrate_sdk_android.runtime.metadata.storageKey
import io.novasama.substrate_sdk_android.runtime.metadata.storageKeys
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.bindNumberOrZero
import io.paritytech.polkadotapp.chains.storage.source.multi.MultiQueryBuilder
import io.paritytech.polkadotapp.chains.storage.source.multi.MultiQueryBuilderImpl
import io.paritytech.polkadotapp.chains.util.decode
import io.paritytech.polkadotapp.chains.util.defaultValue
import io.paritytech.polkadotapp.chains.util.storageKeyWith
import io.paritytech.polkadotapp.common.data.substrate.fromByteArrayOrIncompatible
import io.paritytech.polkadotapp.common.data.substrate.fromHexOrIncompatible
import io.paritytech.polkadotapp.common.data.substrate.incompatible
import io.paritytech.polkadotapp.common.utils.ComponentHolder
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.common.utils.mapValuesDiffed
import io.paritytech.polkadotapp.common.utils.mapValuesNotNull
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import java.math.BigInteger
import io.paritytech.polkadotapp.chains.storage.StorageEntry as StorageEntryValue

abstract class BaseStorageQueryContext(
    override val chainId: ChainId,
    override val runtime: RuntimeSnapshot,
    private val at: BlockHash?,
) : StorageQueryContext {
    protected abstract suspend fun queryKeysByPrefix(
        prefix: String,
        at: BlockHash?,
    ): List<String>

    protected abstract suspend fun queryEntriesByPrefix(
        prefix: String,
        at: BlockHash?,
    ): Map<String, String?>

    protected abstract suspend fun queryKeys(
        keys: List<String>,
        at: BlockHash?,
    ): Map<String, String?>

    protected abstract suspend fun queryKey(
        key: String,
        at: BlockHash?,
    ): String?

    protected abstract fun observeKey(key: String): Flow<StorageUpdate>

    protected abstract suspend fun observeKeys(keys: List<String>): Flow<Map<String, String?>>

    protected abstract suspend fun observeKeysByPrefix(prefix: String): Flow<Map<String, String?>>

    override fun StorageEntry.createStorageKey(vararg keyArguments: Any?): String {
        return storageKeyWith(runtime, keyArguments)
    }

    override suspend fun StorageEntry.keys(vararg prefixArgs: Any?): List<StorageKeyComponents> {
        val prefix = storageKey(runtime, *prefixArgs)

        return queryKeysByPrefix(prefix, at).map { ComponentHolder(splitKey(runtime, it)) }
    }

    override suspend fun <K, V> StorageEntry.entries(
        vararg prefixArgs: Any?,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinderWithKey<K, V>,
        recover: (exception: Exception, rawValue: String?) -> Unit,
    ): Map<K, V> {
        val prefix = storageKey(runtime, *prefixArgs)

        val entries = queryEntriesByPrefix(prefix, at)

        return applyMappersToEntries(
            entries = entries,
            storageEntry = this,
            keyExtractor = keyExtractor,
            binding = binding,
            recover = recover
        )
    }

    override suspend fun <K, V> StorageEntry.entries(
        keysArguments: List<List<Any?>>,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinderWithKey<K, V>,
        recover: (exception: Exception, rawValue: String?) -> Unit,
    ): Map<K, V> {
        val entries = queryKeysOrDefault(storageKeys(runtime, keysArguments), at)

        return applyMappersToEntries(
            entries = entries,
            storageEntry = this,
            keyExtractor = keyExtractor,
            binding = binding,
            recover = recover
        )
    }

    override suspend fun <K, V> StorageEntry.observeByPrefix(
        vararg prefixArgs: Any?,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinderWithKey<K, V>,
    ): Flow<Map<K, V>> {
        val prefixKey = storageKey(runtime, *prefixArgs)

        return observeKeysByPrefix(prefixKey).decodeChangedEntries(storageEntry = this, keyExtractor, binding)
    }

    override suspend fun <K : Any> StorageEntry.findExistingKeys(
        keysArguments: List<List<Any?>>,
        keyExtractor: (StorageKeyComponents) -> K
    ): Set<K> {
        val storageKeys = storageKeys(runtime, keysArguments)
        return queryKeys(storageKeys, at).mapNotNullTo(mutableSetOf()) { (key, value) ->
            if (value != null) {
                val keyComponents = ComponentHolder(splitKey(runtime, key))
                keyExtractor(keyComponents)
            } else {
                null
            }
        }
    }

    override suspend fun StorageEntry.entriesRaw(vararg prefixArgs: Any?): Map<String, String?> {
        return queryEntriesByPrefix(storageKey(runtime, *prefixArgs), at)
    }

    override suspend fun StorageEntry.entriesRaw(keysArguments: List<List<Any?>>): Map<String, String?> {
        return queryKeysOrDefault(storageKeys(runtime, keysArguments), at)
    }

    override suspend fun Module.palletVersionOrThrow(): Int {
        val manualStorageVersionEntry =
            StorageEntry(
                moduleName = name,
                name = ":__STORAGE_VERSION__:",
                modifier = StorageEntryModifier.Required,
                type = StorageEntryType.Plain(value = u16),
                default = u16.toByteArray(runtime, BigInteger.ZERO),
                documentation = emptyList()
            )

        return manualStorageVersionEntry.query(binding = ::bindNumberOrZero).toInt()
    }

    override suspend fun <V> StorageEntry.query(
        vararg keyArguments: Any?,
        binding: DynamicInstanceBinder<V>,
    ): V {
        val scaleResult = queryRaw(*keyArguments)
        val decoded = decode(scaleResult)

        return binding(decoded)
    }

    override suspend fun StorageEntry.queryRaw(vararg keyArguments: Any?): String? {
        val storageKey = storageKeyWith(runtime, keyArguments)

        return queryKeyOrDefault(storageKey, at)
    }

    override fun <V> StorageEntry.observe(
        vararg keyArguments: Any?,
        binding: DynamicInstanceBinder<V>,
    ): Flow<V> {
        val storageKey = storageKeyWith(runtime, keyArguments)

        return observeKeyOrDefault(storageKey).map { storageUpdate ->
            decodeStorageValue(storageUpdate.value, binding)
        }
    }

    override fun <V> StorageEntry.observeWithRaw(
        vararg keyArguments: Any?,
        binding: DynamicInstanceBinder<V>,
    ): Flow<WithRawValue<V>> {
        val storageKey = storageKeyWith(runtime, keyArguments)

        return observeKeyOrDefault(storageKey).map { storageUpdate ->
            val decoded = decodeStorageValue(storageUpdate.value, binding)

            WithRawValue(
                raw = StorageEntryValue(storageKey, storageUpdate.value),
                chainId = chainId,
                value = decoded,
                at = storageUpdate.at,
            )
        }
    }

    override suspend fun <K, V> StorageEntry.observe(
        keysArguments: List<List<Any?>>,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinderWithKey<K, V>,
    ): Flow<Map<K, V>> {
        if (keysArguments.isEmpty()) return flowOf(emptyMap())

        val storageKeys = storageKeys(runtime, keysArguments)

        return observeKeysOrDefault(storageKeys).decodeChangedEntries(storageEntry = this, keyExtractor, binding)
    }

    @Suppress("OVERRIDE_DEPRECATION", "OverridingDeprecatedMember")
    // TODO apply default storage values to multi calls
    override suspend fun multiInternal(builderBlock: MultiQueryBuilder.() -> Unit): MultiQueryBuilder.Result {
        val builder = MultiQueryBuilderImpl(runtime).apply(builderBlock)

        val keys = builder.keys().flatMap { (_, keys) -> keys }
        val values = queryKeys(keys, at)

        val delegate =
            builder.descriptors().mapValues { (descriptor, keys) ->
                keys.associateBy(
                    keySelector = { key -> descriptor.parseKey(key) },
                    valueTransform = { key -> descriptor.parseValue(values[key]) }
                )
            }

        return MultiQueryResult(delegate)
    }

    override suspend fun <V> Constant.getAs(binding: DynamicInstanceBinder<V>): V {
        val rawValue = type!!.fromByteArrayOrIncompatible(value, runtime)

        return binding(rawValue)
    }

    private fun <V> StorageEntry.decodeStorageValue(
        scale: String?,
        binding: DynamicInstanceBinder<V>,
    ): V {
        val dynamicInstance = scale?.let {
            type.value?.fromHex(runtime, scale)
        }

        return binding(dynamicInstance)
    }

    private fun <K, V> applyMappersToEntries(
        entries: Map<String, String?>,
        storageEntry: StorageEntry,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinderWithKey<K, V>,
        recover: (exception: Exception, rawValue: String?) -> Unit = { exception, _ -> throw exception },
    ): Map<K, V> {
        val returnType = storageEntry.type.value ?: incompatible()

        return entries.mapKeys { (key, _) ->
            val keyComponents = ComponentHolder(storageEntry.splitKey(runtime, key))

            keyExtractor(keyComponents)
        }.mapValuesNotNull { (key, value) ->
            try {
                val decoded = value?.let { returnType.fromHexOrIncompatible(value, runtime) }
                binding(decoded, key)
            } catch (e: Exception) {
                recover(e, value)
                null
            }
        }
    }

    // A subscription re-emits every key on each change, so only the entries whose raw value moved are decoded again.
    private fun <K, V> Flow<Map<String, String?>>.decodeChangedEntries(
        storageEntry: StorageEntry,
        keyExtractor: (StorageKeyComponents) -> K,
        binding: DynamicInstanceBinderWithKey<K, V>,
    ): Flow<Map<K, V>> {
        return mapValuesDiffed { storageKey, value ->
            val returnType = storageEntry.type.value ?: incompatible()
            val key = keyExtractor(ComponentHolder(storageEntry.splitKey(runtime, storageKey)))
            val decoded = value?.let { returnType.fromHexOrIncompatible(it, runtime) }

            DecodedEntry(key, binding(decoded, key))
        }.map { entries ->
            entries.values.associate { it.key to it.value }.filterNotNull()
        }
    }

    private class DecodedEntry<K, V>(val key: K, val value: V)

    @JvmInline
    private value class MultiQueryResult(val delegate: Map<MultiQueryBuilder.Descriptor<*, *>, Map<Any?, Any?>>) :
        MultiQueryBuilder.Result {
        @Suppress("UNCHECKED_CAST")
        override fun <K, V> get(descriptor: MultiQueryBuilder.Descriptor<K, V>): Map<K, V> {
            return delegate.getValue(descriptor) as Map<K, V>
        }
    }

    private suspend fun StorageEntry.queryKeyOrDefault(
        key: String,
        at: BlockHash?,
    ): String? {
        return queryKey(key, at) ?: defaultValue()
    }

    private suspend fun StorageEntry.queryKeysOrDefault(
        keys: List<String>,
        at: BlockHash?,
    ): Map<String, String?> {
        val directResult = queryKeys(keys, at)
        return applyDefaultsToMap(directResult)
    }

    private suspend fun StorageEntry.observeKeysOrDefault(keys: List<String>): Flow<Map<String, String?>> {
        return observeKeys(keys).map { applyDefaultsToMap(it) }
    }

    private fun StorageEntry.applyDefaultsToMap(keyToValue: Map<String, String?>): Map<String, String?> {
        return if (modifier == StorageEntryModifier.Default) {
            keyToValue.mapValues { (_, value) -> value ?: default.toHexString() }
        } else {
            keyToValue
        }
    }

    private fun StorageEntry.observeKeyOrDefault(key: String): Flow<StorageUpdate> {
        return observeKey(key).map { storageUpdate ->
            storageUpdate.getOrDefault()
        }
    }

    context(storageEntry: StorageEntry)
    private fun StorageUpdate.getOrDefault() = if (value == null) {
        StorageUpdate(
            value = with(storageEntry) { defaultValue() },
            at = at
        )
    } else this

    protected class StorageUpdate(
        val value: String?,
        // Might be null in case the source does not support identifying the block at which value was changed
        val at: BlockHash?
    )
}
