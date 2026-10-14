package io.paritytech.polkadotapp.common.presentation.search

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.utils.SizedContainer
import io.paritytech.polkadotapp.common.utils.isEmpty
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

@Immutable
sealed interface SearchState<out T : SizedContainer> {
    data object Initial : SearchState<Nothing>
    data object Loading : SearchState<Nothing>
    data object Empty : SearchState<Nothing>
    data class Error(val exception: Throwable) : SearchState<Nothing>
    data class Loaded<T : SizedContainer>(val results: T) : SearchState<T>
}

fun <T : SizedContainer> T.toSearchState(): SearchState<T> {
    return if (isEmpty()) SearchState.Empty else SearchState.Loaded(this)
}

inline fun <T : SizedContainer, R : SizedContainer> SearchState<T>.map(mapper: (T) -> R): SearchState<R> {
    return when (this) {
        is SearchState.Loaded -> mapper(results).toSearchState()
        is SearchState.Initial, is SearchState.Loading, is SearchState.Empty, is SearchState.Error -> this
    }
}

inline fun <T : SizedContainer, R : SizedContainer> Flow<SearchState<T>>.mapSearchResults(
    crossinline mapper: suspend (T) -> R
): Flow<SearchState<R>> {
    return map { state -> state.map { mapper(it) } }
}
