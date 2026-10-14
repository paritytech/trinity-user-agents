package io.paritytech.polkadotapp.common.utils

import androidx.fragment.app.Fragment
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toPersistentList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.channels.produce
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transform
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.withIndex
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import kotlin.coroutines.coroutineContext
import kotlin.experimental.ExperimentalTypeInference
import kotlin.time.Duration
import kotlin.time.TimeSource

fun <T> Flow<T>.inBackground() = flowOn(Dispatchers.Default)

inline fun <T, R> Flow<List<T>>.mapList(crossinline mapper: suspend (T) -> R) =
    map { list ->
        list.map { item -> mapper(item) }
    }

inline fun <T, R> Flow<Result<T>>.mapResult(crossinline mapper: suspend (T) -> R) =
    map { result ->
        result.mapCatching { item -> mapper(item) }
    }

inline fun <T1, T2, R> Flow<Result<T1>>.combineResult(
    flow: Flow<Result<T2>>,
    crossinline transform: suspend (a: T1, b: T2) -> R
) = combine(flow) { aResult, bResult ->
    aResult.flatMap { a ->
        bResult.map { b ->
            transform(a, b)
        }
    }
}

@OptIn(FlowPreview::class)
fun <T> Flow<T>.debounceIndexed(duration: (index: Int, T) -> Duration): Flow<T> {
    return withIndex().debounce { (index, item) -> duration(index, item) }.map { it.value }
}

/**
 * Emits first element from upstream and then emits last element emitted by upstream during specified time window
 *
 * ```
 * flow {
 *  for (num in 1..15) {
 *      emit(num)
 *      delay(25)
 *  }
 * }.throttleLatest(100)
 *  .onEach { println(it) }
 *  .collect()  // Prints 1, 5, 9, 13, 15
 *
 * ```
 */
fun <T> Flow<T>.throttleLatest(duration: Duration): Flow<T> = conflate().transform { value ->
    emit(value)
    delay(duration)
}

/**
 * Emits `LoadingState.Loading` upon receiving an item from upstream
 * Nothing is emitted until first emission from upstream
 */
fun <T, R> Flow<T>.withMapLoading(source: suspend (T) -> Result<R>): Flow<LoadingState<R>> {
    return transformLatest { item ->
        emit(LoadingState.Loading)

        source(item)
            .onSuccess { emit(LoadingState.Loaded(it)) }
            .onFailure { emit(LoadingState.Error(it)) }
    }
}

fun <T> Flow<Result<T?>>.filterResultSuccess(): Flow<T?> =
    filter { it.isSuccess }.map { it.getOrNull() }

fun <T> Flow<Result<T?>>.filterResultSuccessNotNull(): Flow<T> =
    mapNotNull { result -> result.getOrNull() }

fun <T> Flow<Result<T>>.logFailure(message: String? = null): Flow<Result<T>> = onEach { result ->
    result.onFailure { throwable ->
        Timber.e(throwable, message)
    }
}

suspend inline fun Flow<Boolean?>.awaitTrue() {
    first { it == true }
}

inline fun <T, R> Flow<List<T>>.mapListNotNull(crossinline mapper: suspend (T) -> R?) =
    map { list ->
        list.mapNotNull { item -> mapper(item) }
    }

fun <T : Identifiable> Flow<List<T>>.diffed(): Flow<CollectionDiffer.Diff<T>> {
    return zipWithPrevious().map { (previous, new) ->
        CollectionDiffer.findDiff(
            newItems = new,
            oldItems = previous.orEmpty(),
            forceUseNewItems = false
        )
    }
}

fun <T1, T2> combineToPair(flow1: Flow<T1>, flow2: Flow<T2>): Flow<Pair<T1, T2>> =
    combine(flow1, flow2, ::Pair)

fun <T1, T2, T3> combineToTriple(
    flow1: Flow<T1>,
    flow2: Flow<T2>,
    flow3: Flow<T3>
): Flow<Triple<T1, T2, T3>> =
    combine(flow1, flow2, flow3, ::Triple)

fun <T> Flow<T>.gate(flag: Flow<Boolean>) = combineToPair(this, flag)
    .filter { it.second }
    .map { it.first }

fun <T> Flow<T>.reevaluate(trigger: Flow<*>): Flow<T> =
    combine(this, trigger.map { }.onStart { emit(Unit) }) { value, _ -> value }

fun <T> Flow<T>.zipWithPrevious(): Flow<Pair<T?, T>> =
    flow {
        var current: T? = null

        collect {
            emit(current to it)

            current = it
        }
    }

fun <T> Flow<T>.filterWithPrevious(filter: (T?, T) -> Boolean): Flow<T> = zipWithPrevious()
    .filter { filter(it.first, it.second) }
    .map { it.second }

fun <T> Flow<T>.filterWithPreviousIgnoreFirst(filter: (T, T) -> Boolean): Flow<T> {
    return filterWithPrevious { old, new ->
        if (old != null) {
            filter(old, new)
        } else {
            false
        }
    }
}

/** Awaits the first emission that is an instance of [T] and returns it as [T]. */
suspend inline fun <reified T> Flow<*>.firstIsInstance(): T {
    return first { it is T } as T
}

fun <T> Any.asFlow() = flowOf { this }

fun <T> flowOf(producer: suspend () -> T) =
    flow {
        emit(producer())
    }

inline fun <T> flowOfAll(crossinline producer: suspend () -> Flow<T>): Flow<T> =
    flow {
        emitAll(producer())
    }

fun <T : Identifiable, R> Flow<List<T>>.transformLatestDiffed(transform: suspend FlowCollector<R>.(value: T) -> Unit): Flow<R> =
    channelFlow {
        val parentScope = CoroutineScope(coroutineContext)
        val itemScopes = mutableMapOf<String, CoroutineScope>()

        diffed().onEach { diff ->
            diff.removed.forEach { removedItem ->
                itemScopes.removeAndCancel(removedItem.identifier)
            }

            diff.newOrUpdated.forEach { newOrUpdatedItem ->
                itemScopes.removeAndCancel(newOrUpdatedItem.identifier)

                val chainScope = parentScope.childScope(supervised = false)
                itemScopes[newOrUpdatedItem.identifier] = chainScope

                chainScope.launch {
                    transform(SendingCollector(this@channelFlow), newOrUpdatedItem)
                }
            }
        }.launchIn(parentScope)
    }

@Suppress("UNCHECKED_CAST")
@OptIn(ExperimentalTypeInference::class)
inline fun <reified T, reified R> Flow<Result<T>>.transformResult(
    @BuilderInference crossinline transform: suspend FlowCollector<R>.(value: T) -> Unit
): Flow<Result<R>> {
    return transform { upstream ->
        upstream.onFailure {
            emit(upstream as Result<R>)
        }.onSuccess {
            val innerCollector = FlowCollector<R> {
                emit(Result.success(it))
            }

            runCatching {
                transform(innerCollector, it)
            }.onFailure {
                if (it is CancellationException) {
                    throw it
                }

                emit(Result.failure(it))
            }
        }
    }
}

private fun <K> MutableMap<K, CoroutineScope>.removeAndCancel(key: K) {
    remove(key)?.also(CoroutineScope::cancel)
}

private class SendingCollector<T>(
    private val channel: SendChannel<T>
) : FlowCollector<T> {
    override suspend fun emit(value: T): Unit = channel.send(value)
}

@Suppress("FunctionName")
fun <T> OneShotEventChannel() = Channel<T>(Channel.CONFLATED)

context(scope: CoroutineScope)
fun <T> Flow<T>.share(started: SharingStarted = SharingStarted.Eagerly) =
    shareIn(scope, started = started, replay = 1)

context(scope: CoroutineScope)
fun <T> Flow<T>.shareLazily() =
    shareIn(scope, started = SharingStarted.Lazily, replay = 1)

context(scope: CoroutineScope)
fun <T> Flow<T>.shareInBackground(started: SharingStarted = SharingStarted.Eagerly) =
    inBackground().share(started)

context(scope: CoroutineScope)
fun <T> Flow<T>.stateInBackground(
    started: SharingStarted = SharingStarted.Eagerly,
    initialValue: T
) = inBackground().stateIn(scope, started, initialValue)

context(scope: CoroutineScope)
fun <T> Flow<T>.stateInBackgroundWithLoading(
    started: SharingStarted = SharingStarted.Eagerly
) = withLoading()
    .inBackground()
    .stateIn(scope, started, LoadingState.Loading)

context(scope: CoroutineScope)
fun <T> Flow<T>.shareWhileSubscribed() = share(SharingStarted.WhileSubscribed())

context(lifecycleOwner: LifecycleOwner)
fun <V> Flow<V>.observe(collector: suspend (V) -> Unit) {
    lifecycleOwner.lifecycleScope.launch {
        lifecycleOwner.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            collect(collector)
        }
    }
}

context(lifecycleOwner: LifecycleOwner)
fun <V> Flow<V>.observeWhenCreated(collector: suspend (V) -> Unit) {
    lifecycleOwner.lifecycleScope.launch {
        lifecycleOwner.repeatOnLifecycle(Lifecycle.State.CREATED) {
            collect(collector)
        }
    }
}

context(fragment: Fragment)
fun <V> Flow<V>.observeWhenVisible(collector: suspend (V) -> Unit) {
    fragment.viewLifecycleOwner.lifecycleScope.launch {
        fragment.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            collect(collector)
        }
    }
}

context(fragment: Fragment)
fun <V> Flow<V>.observeWhenStarted(collector: suspend (V) -> Unit) {
    fragment.viewLifecycleOwner.lifecycleScope.launch {
        fragment.viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) {
            collect(collector)
        }
    }
}

context(fragment: Fragment)
fun <V> Flow<V>.collectWhenVisible() {
    fragment.viewLifecycleOwner.lifecycleScope.launch {
        fragment.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            collect()
        }
    }
}

/**
 * Modifies flow so that it firstly emits [LoadingState.Loading] state.
 * Then emits each element from upstream wrapped into [LoadingState.Loaded] state.
 * If exception occurs, emits [LoadingState.Error] state.
 */
fun <T> Flow<Result<T>>.withLoading(failureLog: String? = null): Flow<LoadingState<T>> {
    return (if (failureLog != null) logFailure(failureLog) else this)
        .map { result ->
            result.fold(
                onSuccess = { LoadingState.Loaded(it) },
                onFailure = { LoadingState.Error(it) }
            )
        }
        .onStart { emit(LoadingState.Loading) }
}

@JvmName("withCatchingLoading")
fun <T> Flow<T>.withLoading(failureLog: String? = null): Flow<LoadingState<T>> {
    return wrapIntoResult()
        .logFailure(failureLog)
        .withLoading()
}

fun <T> List<Flow<T>>.mergeIfMultiple(): Flow<T> = when (size) {
    0 -> emptyFlow()
    1 -> first()
    else -> merge()
}

fun <T> Flow<T>.wrapIntoResult(): Flow<Result<T>> {
    return map(Result.Companion::success)
        .catch { emit(Result.failure(it)) }
}

fun Flow<*>.mapToUnit(): Flow<Unit> = map {}

inline fun <T, R> Flow<T>.concurrentMap(crossinline transform: suspend (T) -> R): Flow<R> =
    channelFlow {
        collect {
            launch { send(transform(it)) }
        }
    }

/**
 * Similar to [Flow.takeWhile] but emits last element too
 */
fun <T> Flow<T>.takeWhileInclusive(predicate: suspend (T) -> Boolean) = transformWhile {
    emit(it)

    predicate(it)
}

fun MutableStateFlow<Boolean>.enable() {
    value = true
}

fun MutableStateFlow<Boolean>.disable() {
    value = false
}

fun MutableStateFlow<Boolean>.toggle() {
    value = !value
}

fun <T> singleReplaySharedFlow() = MutableSharedFlow<T>(replay = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

inline fun <reified T> MutableStateFlow<List<T>>.updateItem(
    condition: (T) -> Boolean,
    updater: (T) -> T
) {
    update {
        val index = it.indexOfFirst(condition)

        if (index >= 0) {
            val mutableList = it.toMutableList()
            mutableList[index] = updater(mutableList[index])

            mutableList
        } else it
    }
}

@JvmName("updateItemImmutable")
inline fun <reified T> MutableStateFlow<ImmutableList<T>>.updateItem(
    condition: (T) -> Boolean,
    updater: (T) -> T
) {
    update {
        val index = it.indexOfFirst(condition)

        if (index >= 0) {
            it.toPersistentList().set(index, updater(it[index]))
        } else it
    }
}

inline fun <reified T> MutableStateFlow<List<T>>.removeItems(
    condition: (T) -> Boolean
) {
    update { it.filterNot(condition) }
}

fun <T> Flow<T>.chunkedByInactivity(timeout: Duration): Flow<List<T>> = channelFlow {
    val currentChunk = mutableListOf<T>()
    var timeoutJob: Job? = null

    collect { value ->
        timeoutJob?.cancel()

        currentChunk.add(value)

        timeoutJob = launch {
            delay(timeout)
            send(ArrayList(currentChunk))

            currentChunk.clear()
        }
    }
}

fun <T> Flow<T>.chunked(
    maxSize: Int,
    maxDelay: Duration,
): Flow<List<T>> = channelFlow {
    val upstream = produce { collect { send(it) } }
    val buffer = mutableListOf<T>()

    try {
        while (true) {
            val start = TimeSource.Monotonic.markNow()
            buffer += upstream.receiveCatching().getOrNull() ?: break

            val elapsed = start.elapsedNow()
            if (elapsed < maxDelay) {
                withTimeoutOrNull(maxDelay - elapsed) {
                    while (buffer.size < maxSize) {
                        buffer += upstream.receiveCatching().getOrNull() ?: return@withTimeoutOrNull
                    }
                }
            }

            send(buffer.toList())
            buffer.clear()
        }
    } finally {
        if (buffer.isNotEmpty()) send(buffer.toList())
    }
}

/**
 * Groups items emitted during each sampling window into a List<T>.
 * Each emitted list contains items received since the previous tick.
 *
 * Notes:
 * - Empty windows emit nothing by default (emitEmpty=false).
 * - If the downstream is slow, results are buffered by the channelFlow.
 */
fun <T> Flow<T>.sampleWindow(
    window: Duration,
    emitEmpty: Boolean = false
): Flow<List<T>> = channelFlow {
    val buffer = ArrayList<T>()
    val mutex = Mutex()

    // Producer: collect upstream items into buffer
    val collectorJob = launch {
        collect { value ->
            mutex.withLock { buffer.add(value) }
        }
    }

    // Ticker: flush buffer every window
    val tickerJob = launch {
        while (true) {
            delay(window)
            val batch: List<T> = mutex.withLock {
                if (buffer.isEmpty()) emptyList() else buffer.toList().also { buffer.clear() }
            }
            if (emitEmpty || batch.isNotEmpty()) {
                send(batch)
            }
        }
    }

    // Cancel children when downstream cancels
    awaitClose {
        collectorJob.cancel()
        tickerJob.cancel()
    }
}

fun <T> Collection<Flow<T>>.accumulate(): Flow<List<T>> {
    return accumulate(*this.toTypedArray())
}

fun <T> accumulate(vararg flows: Flow<T>): Flow<List<T>> {
    val flowsList = flows.mapIndexed { index, flow -> flow.map { index to flow } }
    val resultOfFlows = MutableList<T?>(flowsList.size) { null }
    val lock = Mutex()

    return flowsList
        .merge()
        .map {
            lock.withLock { resultOfFlows[it.first] = it.second.first() }
            resultOfFlows.filterNotNull().toList()
        }
}

fun <T> accumulateFlatten(vararg flows: Flow<List<T>>): Flow<List<T>> {
    return accumulate(*flows).map { it.flatten() }
}

inline fun <T> withFlowScope(crossinline block: suspend (scope: CoroutineScope) -> Flow<T>): Flow<T> {
    return flowOfAll {
        val flowScope = CoroutineScope(coroutineContext)

        block(flowScope)
    }
}

fun <K, V> List<Flow<Pair<K, V>>>.toMultiSubscription(expectedSize: Int): Flow<Map<K, V>> {
    return mergeIfMultiple()
        .runningFold(emptyMap<K, V>()) { accumulator, tokenIdWithBalance ->
            accumulator + tokenIdWithBalance
        }
        .filter { it.size == expectedSize }
}

/**
 * Re-applies [transform] only to the entries whose value differs from the previous emission; the rest keep their
 * previous result. Entries missing from the current emission are dropped.
 */
fun <K, V, R> Flow<Map<K, V>>.mapValuesDiffed(transform: suspend (key: K, value: V) -> R): Flow<Map<K, R>> = flow {
    var previous = emptyMap<K, DiffedValue<V, R>>()

    collect { current ->
        val next = LinkedHashMap<K, DiffedValue<V, R>>(current.size)

        current.forEach { (key, value) ->
            val cached = previous[key]

            next[key] = if (cached != null && cached.input == value) cached else DiffedValue(value, transform(key, value))
        }

        previous = next

        emit(next.mapValues { (_, diffed) -> diffed.output })
    }
}

private class DiffedValue<V, R>(val input: V, val output: R)

@Suppress("UNCHECKED_CAST")
fun <T1, T2, T3, T4, T5, T6, R> combine(
    flow: Flow<T1>,
    flow2: Flow<T2>,
    flow3: Flow<T3>,
    flow4: Flow<T4>,
    flow5: Flow<T5>,
    flow6: Flow<T6>,
    transform: suspend (T1, T2, T3, T4, T5, T6) -> R
): Flow<R> = combine(flow, flow2, flow3, flow4, flow5, flow6) { args: Array<*> ->
    transform(
        args[0] as T1,
        args[1] as T2,
        args[2] as T3,
        args[3] as T4,
        args[4] as T5,
        args[5] as T6
    )
}

@Suppress("UNCHECKED_CAST")
fun <T1, T2, T3, T4, T5, T6, T7, R> combine(
    flow: Flow<T1>,
    flow2: Flow<T2>,
    flow3: Flow<T3>,
    flow4: Flow<T4>,
    flow5: Flow<T5>,
    flow6: Flow<T6>,
    flow7: Flow<T7>,
    transform: suspend (T1, T2, T3, T4, T5, T6, T7) -> R
): Flow<R> = combine(flow, flow2, flow3, flow4, flow5, flow6, flow7) { args: Array<*> ->
    transform(
        args[0] as T1,
        args[1] as T2,
        args[2] as T3,
        args[3] as T4,
        args[4] as T5,
        args[5] as T6,
        args[6] as T7
    )
}

inline fun <reified T> Iterable<Flow<T>>.combine(): Flow<List<T>> {
    return combineIdentity(this)
}

inline fun <reified T> combineIdentity(flows: Iterable<Flow<T>>): Flow<List<T>> {
    return combine(flows) { it.toList() }
}

suspend inline fun MutableSharedFlow<Unit>.emit() = emit(Unit)
