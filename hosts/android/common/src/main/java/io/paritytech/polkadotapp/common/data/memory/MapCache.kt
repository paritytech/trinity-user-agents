package io.paritytech.polkadotapp.common.data.memory

import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.DelicateCoroutinesApi
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

interface MapCache<K, V> {
    suspend fun getOrCompute(key: K): V

    suspend fun clear()
}

/**
 * In-memory cache primitive that caches asynchronously computed value
 * Lifetime of the cache itself is determine by supplied [CoroutineScope]
 */
fun <K, V> MapCache(coroutineScope: CoroutineScope, compute: AsyncCacheCompute<K, V>): MapCache<K, V> {
    return RealMapCache(coroutineScope, compute)
}

/**
 * Specialization of [MapCache] that's cached value is a [SharedFlow] shared in the supplied [coroutineScope]
 */
inline fun <K, V> SharedFlowMapCache(
    coroutineScope: CoroutineScope,
    crossinline compute: suspend (key: K) -> Flow<V>
): MapCache<K, SharedFlow<V>> {
    return MapCache(coroutineScope) { key ->
        compute(key).shareIn(coroutineScope, SharingStarted.Eagerly, replay = 1)
    }
}

typealias AsyncCacheCompute<K, V> = suspend (key: K) -> V

private class RealMapCache<K, V>(
    private val lifetime: CoroutineScope,
    private val compute: AsyncCacheCompute<K, V>,
) : MapCache<K, V> {
    private val mutex = Mutex()
    private val cache = mutableMapOf<K, V>()

    override suspend fun getOrCompute(key: K): V {
        mutex.withLock {
            if (key in cache) return cache.getValue(key)

            return compute(key).also {
                cache[key] = it
            }
        }
    }

    override suspend fun clear() {
        mutex.withLock {
            cache.clear()
        }
    }

    init {
        lifetime.invokeOnCompletion {
            clearCache()
        }
    }

    // GlobalScope job is fine here since it just for clearing the map
    @OptIn(DelicateCoroutinesApi::class)
    private fun clearCache() = GlobalScope.launch {
        mutex.withLock { cache.clear() }
    }
}
