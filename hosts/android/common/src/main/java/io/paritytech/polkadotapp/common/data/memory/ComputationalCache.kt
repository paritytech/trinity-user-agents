package io.paritytech.polkadotapp.common.data.memory

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow

interface ComputationalCache {
    /**
     * Caches [computation] between calls until all supplied [scope]s have been cancelled
     */
    suspend fun <T> useCache(
        key: String,
        scope: CoroutineScope,
        computation: Computation<T>
    ): T

    fun <T> useSharedFlow(
        key: String,
        scope: CoroutineScope,
        flowLazy: Computation<Flow<T>>
    ): Flow<T>
}

context(scope: ComputationalScope)
suspend fun <T> ComputationalCache.useCache(
    key: String,
    computation: Computation<T>
): T = useCache(key, scope, computation)

context(scope: ComputationalScope)
fun <T> ComputationalCache.useSharedFlow(
    key: String,
    flowLazy: Computation<Flow<T>>
): Flow<T> = useSharedFlow(key, scope, flowLazy)

context(scope: ComputationalScope)
fun <T> ComputationalCache.useSharedFlow(
    vararg keyArgs: String,
    flowLazy: Computation<Flow<T>>
): Flow<T> = useSharedFlow(keyArgs.joinToString(separator = ":"), scope, flowLazy)

typealias Computation<T> = suspend (cachingScope: ComputationalScope) -> T
