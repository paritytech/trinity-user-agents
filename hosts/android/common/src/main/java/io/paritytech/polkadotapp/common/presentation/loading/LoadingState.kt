package io.paritytech.polkadotapp.common.presentation.loading

import androidx.compose.runtime.Immutable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.FlowCollector
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull

@Immutable
sealed class LoadingState<out T> {
    companion object;

    data object Loading : LoadingState<Nothing>()

    data class Error(val exception: Throwable) : LoadingState<Nothing>()

    data class Loaded<T>(val data: T) : LoadingState<T>()
}

inline fun <T, V> Flow<LoadingState<T>>.mapLoading(crossinline mapper: suspend (T) -> V): Flow<LoadingState<V>> {
    return map { loadingState -> loadingState.map { mapper(it) } }
}

suspend inline fun <T> Flow<LoadingState<T>>.awaitLoaded(): T {
    return first { it is LoadingState.Loaded }
        .let { (it as LoadingState.Loaded<T>).data }
}

inline fun <T, R> LoadingState<T>.map(mapper: (T) -> R): LoadingState<R> {
    return when (this) {
        is LoadingState.Loading -> this
        is LoadingState.Error -> this
        is LoadingState.Loaded<T> -> LoadingState.Loaded(mapper(data))
    }
}

val <T> LoadingState<T>.dataOrNull: T?
    get() =
        when (this) {
            is LoadingState.Loaded -> this.data
            else -> null
        }

fun <T> loadedNothing(): LoadingState<T?> {
    return LoadingState.Loaded(null)
}

fun LoadingState<*>.isLoading(): Boolean {
    return this is LoadingState.Loading
}

fun LoadingState<*>.isLoaded(): Boolean {
    return this is LoadingState.Loaded
}

suspend inline fun <T> Flow<LoadingState<T>>.firstLoaded(): T {
    return mapNotNull { it.dataOrNull }.first()
}

suspend fun <T> FlowCollector<LoadingState<T>>.emitLoaded(value: T) {
    emit(LoadingState.Loaded(value))
}

suspend fun <T> FlowCollector<LoadingState<T>>.emitLoading() {
    emit(LoadingState.Loading)
}

suspend fun <T> FlowCollector<LoadingState<T>>.emitError(throwable: Throwable) {
    emit(LoadingState.Error(throwable))
}

fun <T> LoadingState.Companion.fromOption(value: T?): LoadingState<T> {
    return if (value != null) {
        LoadingState.Loaded(value)
    } else {
        LoadingState.Loading
    }
}

fun <T> T.asLoaded(): LoadingState.Loaded<T> = LoadingState.Loaded(this)

inline fun <T> LoadingState<T>.onLoaded(action: (T) -> Unit): LoadingState<T> {
    if (this is LoadingState.Loaded) {
        action(data)
    }

    return this
}

inline fun <T> LoadingState<T>.onNotLoaded(action: () -> Unit): LoadingState<T> {
    if (this !is LoadingState.Loaded) {
        action()
    }

    return this
}

inline fun <T> LoadingState<T>.onLoading(action: () -> Unit): LoadingState<T> {
    if (this is LoadingState.Loading) {
        action()
    }

    return this
}

inline fun <T> LoadingState<T>.onError(action: (Throwable) -> Unit): LoadingState<T> {
    if (this is LoadingState.Error) {
        action(exception)
    }

    return this
}
