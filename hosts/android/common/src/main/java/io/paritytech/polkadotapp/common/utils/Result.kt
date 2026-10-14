package io.paritytech.polkadotapp.common.utils

import androidx.work.ListenableWorker
import io.novasama.substrate_sdk_android.extensions.tryFindNonNull
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import timber.log.Timber
import kotlin.contracts.ExperimentalContracts
import kotlin.contracts.InvocationKind
import kotlin.contracts.contract

@Suppress("UNCHECKED_CAST")
inline fun <T> Result<Result<T>>.flatten(): Result<T> {
    return flatMap { it }
}

// TODO this can be implemented more efficiently: currently doing list allocation on each iteration
inline fun <T> List<Result<T>>.flattenResult(): Result<List<T>> {
    return fold(Result.success(emptyList())) { acc, result ->
        combineResults(acc, result) { accSuccess, resultSuccess ->
            accSuccess + resultSuccess
        }
    }
}

@Suppress("UNCHECKED_CAST")
inline fun <T, R> Result<T>.flatMap(transform: (T) -> Result<R>): Result<R> {
    return fold(
        onSuccess = { transform(it) },
        onFailure = { this as Result<R> }
    )
}

inline fun <R> runCancellableCatching(block: () -> R): Result<R> {
    return try {
        Result.success(block())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Throwable) {
        Result.failure(e)
    }
}

fun <T> Result<List<T>>.getOrEmpty(): List<T> {
    return getOrElse { emptyList() }
}

fun <T> Result<T>.logFailure(label: String): Result<T> {
    return onFailure { Timber.e(it, label) }
}

fun <K, V> Result<Map<K, V>>.getOrEmpty(): Map<K, V> {
    return getOrElse { emptyMap() }
}

inline fun <T> Result<T>.flatRecover(recover: (exception: Throwable) -> Result<T>): Result<T> {
    return when (val exception = exceptionOrNull()) {
        null -> this
        else -> recover(exception)
    }
}

inline fun <T, R> Result<List<T>>.mapList(crossinline mapper: (T) -> R) =
    map { list ->
        list.map { item -> mapper(item) }
    }

fun <T> T?.toResult(errorMessage: () -> String = { "Required value was null" }): Result<T> {
    return if (this == null) {
        Result.failure(IllegalStateException(errorMessage()))
    } else {
        Result.success(this)
    }
}

fun <T> Result<Set<T>>.getOrEmpty(): Set<T> {
    return getOrElse { emptySet() }
}

fun <T> Result<T?>.requireNotNull(): Result<T> {
    return mapCatching { requireNotNull(it) }
}

@Suppress("NOTHING_TO_INLINE")
inline fun <T> Result<T>.logSuccess(message: String): Result<T> {
    return onSuccess { Timber.d(message) }
}

fun <T> List<Result<T>>.flatten(): Result<List<T>> {
    if (isEmpty()) {
        return Result.failure(IllegalArgumentException("Cannot flatten empty list of results"))
    }

    val allSuccess = all { it.isSuccess }
    if (allSuccess) {
        val asSuccess = map { it.getOrThrow() }
        return Result.success(asSuccess)
    } else {
        val firstFailure = tryFindNonNull { it.exceptionOrNull() }!!
        return Result.failure(firstFailure)
    }
}

fun List<Result<Unit>>.flattenUnit(): Result<Unit> {
    return flatten().coerceToUnit()
}

inline fun <T : Any> Result<T?>.requireNotNull(error: () -> Throwable): Result<T> {
    return flatMap {
        if (it != null) {
            Result.success(it)
        } else {
            Result.failure(error())
        }
    }
}

inline fun <T : Any, R> Result<T?>.flatMapNotNull(transform: (T) -> Result<R>): Result<R?> {
    return flatMap {
        if (it == null) return@flatMap Result.success(null)

        transform(it)
    }
}

fun <T> Result<T>.coerceToUnit(): Result<Unit> = map { }

fun <T1, T2> combine(result1: Result<T1>, result2: Result<T2>): Result<Pair<T1, T2>> {
    return result1.flatMap { one ->
        result2.map { two ->
            one to two
        }
    }
}

fun <T1, T2, R> combineResults(
    result1: Result<T1>,
    result2: Result<T2>,
    transform: (T1, T2) -> R,
): Result<R> {
    return result1.flatMap { one ->
        result2.map { two ->
            transform(one, two)
        }
    }
}

fun <T1, T2, T3, R> combineResults(
    result1: Result<T1>,
    result2: Result<T2>,
    result3: Result<T3>,
    transform: (T1, T2, T3) -> R,
): Result<R> {
    return result1.flatMap { one ->
        result2.flatMap { two ->
            result3.map { three ->
                transform(one, two, three)
            }
        }
    }
}

fun <T1, T2, T3, T4, R> combineResults(
    result1: Result<T1>,
    result2: Result<T2>,
    result3: Result<T3>,
    result4: Result<T4>,
    transform: (T1, T2, T3, T4) -> R,
): Result<R> {
    return result1.flatMap { one ->
        result2.flatMap { two ->
            result3.flatMap { three ->
                result4.map { four ->
                    transform(one, two, three, four)
                }
            }
        }
    }
}

inline fun <T, R> List<T>.foldResult(
    initial: R,
    fold: (acc: R, element: T) -> Result<R>,
): Result<R> {
    return fold(Result.success(initial)) { acc, element ->
        acc.flatMap { fold(it, element) }
    }
}

inline fun <T : Any, R> Result<T?>.mapNotNull(transform: (T) -> R): Result<R?> {
    return map { it?.let(transform) }
}

fun Result<*>.toWorkerResult(retryOnFailure: Boolean = false) = fold(
    onSuccess = { ListenableWorker.Result.success() },
    onFailure = {
        if (retryOnFailure) {
            ListenableWorker.Result.retry()
        } else {
            ListenableWorker.Result.failure()
        }
    }
)

fun <T> Flow<Result<T>>.unwrapResultOrDefault(default: T): Flow<T> {
    return map { it.getOrDefault(default) }
}

fun <T> T.intoSuccessResult(): Result<T> {
    return Result.success(this)
}

@OptIn(ExperimentalContracts::class)
inline fun <T> Result<T>.mapError(transform: (throwable: Throwable) -> Throwable): Result<T> {
    contract {
        callsInPlace(transform, InvocationKind.AT_MOST_ONCE)
    }
    return when (val exception = this.exceptionOrNull()) {
        null -> this
        else -> Result.failure(transform(exception))
    }
}

inline fun <T, reified E : Throwable> Result<T>.mapErrorNotInstance(
    transform: (throwable: Throwable) -> Throwable,
): Result<T> {
    return mapError { if (it is E) it else transform(it) }
}

inline fun <T, reified E : Throwable> Result<T>.mapErrorInstance(
    transform: (throwable: E) -> Throwable,
): Result<T> {
    return mapError { if (it is E) transform(it) else it }
}
