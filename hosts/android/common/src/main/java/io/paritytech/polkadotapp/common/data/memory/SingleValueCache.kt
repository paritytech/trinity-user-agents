package io.paritytech.polkadotapp.common.data.memory

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

interface SingleValueCache<T> {
    suspend operator fun invoke(): T
}

typealias SingleValueCacheCompute<T> = suspend () -> T

fun <T> SingleValueCache(compute: SingleValueCacheCompute<T>): SingleValueCache<T> {
    return RealSingleValueCache(compute)
}

/**
 * Runs the cache over a throwing computation. A throw leaves the cache empty, so a failed attempt is
 * retried on the next call instead of being pinned for the rest of the session.
 */
suspend fun <T> SingleValueCache<T>.getCatching(): Result<T> {
    return runCancellableCatching { invoke() }
}

private class RealSingleValueCache<T>(
    private val compute: SingleValueCacheCompute<T>,
) : SingleValueCache<T> {
    private val mutex = Mutex()
    private var cache: Any? = NULL

    @Suppress("UNCHECKED_CAST")
    override suspend operator fun invoke(): T {
        mutex.withLock {
            if (cache === NULL) {
                cache = compute()
            }

            return cache as T
        }
    }

    private object NULL
}
