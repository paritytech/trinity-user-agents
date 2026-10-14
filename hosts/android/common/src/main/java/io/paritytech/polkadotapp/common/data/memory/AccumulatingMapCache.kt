package io.paritytech.polkadotapp.common.data.memory

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap

/**
 * Caches already retrieved keys and upon subsequent requests only requests missing keys.
 *
 * [K] should be a valid type for a HashMap key.
 */
interface AccumulatingMapCache<K, V> {
    /**
     * @return [Result.success] if all keys have been fetched successfully, [Result.failure] otherwise
     */
    suspend fun get(keys: Iterable<K>): Result<Map<K, V>>

    /**
     * Returns already cached values for [keys]. null in case there isn't sufficient cache data to cover [keys]
     */
    suspend fun peek(keys: Iterable<K>): Map<K, V>?
}

fun <K, V> AccumulatingMapCache(resolveItems: suspend (List<K>) -> Result<Map<K, V>>): AccumulatingMapCache<K, V> {
    return RealAccumulatingMapCache(resolveItems)
}

internal class RealAccumulatingMapCache<K, V>(
    private val resolveItems: suspend (List<K>) -> Result<Map<K, V>>
) : AccumulatingMapCache<K, V> {
    // Concurrent rather than left to the mutex: [peek] reads without taking it, and has to — the lock is
    // held across resolveItems, so a peek that waited for it would be waiting for the network.
    private val cache = ConcurrentHashMap<K, V>()

    // Still serializes [get] so concurrent callers missing the same keys resolve them once.
    private val cacheAccessMutex = Mutex()

    override suspend fun get(keys: Iterable<K>): Result<Map<K, V>> = cacheAccessMutex.withLock {
        val missingKeys = keys - cache.keys
        if (missingKeys.isEmpty()) {
            return@withLock Result.success(keysFromSaturatedCache(keys))
        }

        resolveItems(missingKeys.toList())
            .onFailure { Timber.e(it, "Failed to get items") }
            .map { result ->
                cache.putAll(result)
                keysFromSaturatedCache(keys)
            }
    }

    override suspend fun peek(keys: Iterable<K>): Map<K, V>? {
        // Entries are only ever added, so a key present for this check is still present when it is read.
        // containsKey, not `in`: on a ConcurrentHashMap the operator would resolve to containsValue.
        if (keys.any { !cache.containsKey(it) }) return null

        return keysFromSaturatedCache(keys)
    }

    private fun keysFromSaturatedCache(keys: Iterable<K>): Map<K, V> {
        return keys.associateWith { key -> cache.getValue(key) }
    }
}
