package io.paritytech.polkadotapp.chains.storage.source.query

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.storage.StorageEntry
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

class LocalStorageQueryContext(
    private val storageCache: StorageCache,
    chainId: ChainId,
    at: BlockHash?,
    runtime: RuntimeSnapshot,
) : BaseStorageQueryContext(chainId, runtime, at) {
    override suspend fun queryKeysByPrefix(
        prefix: String,
        at: BlockHash?,
    ): List<String> {
        return storageCache.getKeys(prefix, chainId)
    }

    override suspend fun queryEntriesByPrefix(
        prefix: String,
        at: BlockHash?,
    ): Map<String, String?> {
        return observeKeysByPrefix(prefix)
            .filter { it.isNotEmpty() }
            .first()
    }

    override suspend fun queryKeys(
        keys: List<String>,
        at: BlockHash?,
    ): Map<String, String?> {
        return storageCache.getEntries(keys, chainId).toMap()
    }

    override suspend fun queryKey(
        key: String,
        at: BlockHash?,
    ): String? {
        return storageCache.getEntry(key, chainId).content
    }

    override fun observeKey(key: String): Flow<StorageUpdate> {
        return storageCache.observeEntry(key, chainId).map {
            StorageUpdate(it.content, at = null)
        }
    }

    override suspend fun observeKeys(keys: List<String>): Flow<Map<String, String?>> {
        return storageCache.observeEntries(keys, chainId).map { it.toMap() }
    }

    override suspend fun observeKeysByPrefix(prefix: String): Flow<Map<String, String?>> {
        return storageCache.observeEntries(prefix, chainId)
            .map { storageEntries ->
                storageEntries.associateBy(
                    keySelector = StorageEntry::storageKey,
                    valueTransform = StorageEntry::content
                )
            }
    }

    private fun List<StorageEntry>.toMap() =
        associateBy(
            keySelector = StorageEntry::storageKey,
            valueTransform = StorageEntry::content
        )
}
