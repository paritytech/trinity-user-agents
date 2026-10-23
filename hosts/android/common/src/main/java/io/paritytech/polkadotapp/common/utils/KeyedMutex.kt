package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap

/** A [Mutex] per key, created on first use and kept for the lifetime of the owner. */
class KeyedMutex<K : Any> {
    private val mutexes = ConcurrentHashMap<K, Mutex>()

    suspend fun <T> withLock(key: K, action: suspend () -> T): T {
        return mutexes.getOrPut(key) { Mutex() }.withLock { action() }
    }
}
