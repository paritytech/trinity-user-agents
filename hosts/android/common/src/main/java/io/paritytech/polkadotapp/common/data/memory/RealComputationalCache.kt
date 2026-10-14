package io.paritytech.polkadotapp.common.data.memory

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.invokeOnCompletion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

private typealias Awaitable<T> = suspend () -> T
private typealias AwaitableConstructor<T> = suspend (aggregateScope: ComputationalScope) -> Awaitable<T>

@Singleton
internal class RealComputationalCache @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
) : ComputationalCache {
    private val cancellationContext = CoroutineScope(coroutineDispatchers.computation)

    private class Entry(
        val dependents: MutableSet<CoroutineScope>,
        val aggregateScope: CoroutineScope,
        val awaitable: Awaitable<Any?>
    )

    private val memory = mutableMapOf<String, Entry>()
    private val mutex = Mutex()

    override suspend fun <T> useCache(
        key: String,
        scope: CoroutineScope,
        computation: Computation<T>
    ): T = withContext(coroutineDispatchers.computation) {
        useCacheInternal(key, scope) { aggregateScope ->
            val deferred = async { computation(aggregateScope) }

            return@useCacheInternal { deferred.await() }
        }
    }

    override fun <T> useSharedFlow(
        key: String,
        scope: CoroutineScope,
        flowLazy: Computation<Flow<T>>
    ): Flow<T> {
        return flowOfAll {
            useCacheInternal(key, scope) { aggregateScope ->
                val inner = flowOfAll { flowLazy(aggregateScope) }
                    .flowOn(coroutineDispatchers.computation)
                    .shareIn(aggregateScope, SharingStarted.Eagerly, replay = 1)

                return@useCacheInternal { inner }
            }
        }
    }

    @Suppress("UNCHECKED_CAST")
    private suspend fun <T> useCacheInternal(
        key: String,
        scope: CoroutineScope,
        cachedAction: AwaitableConstructor<T>
    ): T {
        val awaitable = mutex.withLock {
            if (key in memory) {
                Timber.d("Key $key requested - already present")

                val entry = memory.getValue(key)

                entry.dependents += scope

                entry.awaitable
            } else {
                Timber.d("Key $key requested - creating new operation")

                val aggregateScope = CoroutineScope(coroutineDispatchers.computation)
                val awaitable = cachedAction(ComputationalScope(aggregateScope))

                memory[key] = Entry(dependents = mutableSetOf(scope), aggregateScope, awaitable)

                awaitable
            }
        }

        scope.invokeOnCompletion {
            cancellationContext.launch {
                mutex.withLock {
                    memory[key]?.let { entry ->
                        entry.dependents -= scope

                        if (entry.dependents.isEmpty()) {
                            Timber.d("Key $key - last scope cancelled")
                            memory.remove(key)

                            entry.aggregateScope.cancel()
                        } else {
                            Timber.d("Key $key - scope cancelled, ${entry.dependents.size} remaining")
                        }
                    }
                }
            }
        }

        return awaitable() as T
    }
}
