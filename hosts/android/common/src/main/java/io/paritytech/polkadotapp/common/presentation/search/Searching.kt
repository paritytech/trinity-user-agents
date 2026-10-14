package io.paritytech.polkadotapp.common.presentation.search

import io.paritytech.polkadotapp.common.utils.SizedContainer
import io.paritytech.polkadotapp.common.utils.SizedList
import io.paritytech.polkadotapp.common.utils.SizedMap
import io.paritytech.polkadotapp.common.utils.toSizedList
import io.paritytech.polkadotapp.common.utils.toSizedMap
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.withIndex
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

private val DEFAULT_SEARCH_DEBOUNCE = 300.milliseconds

/**
 * Drives a searching pipeline from a flow of optional inputs.
 *
 * - `null` input -> [SearchState.Initial].
 * - Non-null input -> emits [SearchState.Loading] immediately, waits [debounce], then runs [search].
 *   While the upstream keeps emitting, the in-flight delay/search is cancelled and restarted,
 *   so requests fire only after the input settles.
 * - The very first upstream value is not debounced: it is the initial state of the input rather than
 *   the user typing, so there is nothing to wait for.
 */
@OptIn(ExperimentalCoroutinesApi::class)
fun <T : Any, C : SizedContainer> Flow<T?>.withSizedSearching(
    debounce: Duration = DEFAULT_SEARCH_DEBOUNCE,
    search: suspend (T) -> Result<C>
): Flow<SearchState<C>> {
    return withIndex().transformLatest { (index, input) ->
        if (input == null) {
            emit(SearchState.Initial)
        } else {
            emit(SearchState.Loading)
            if (index > 0) delay(debounce)
            emit(search(input).toSearchState())
        }
    }
}

/**
 * Variant of [withSizedSearching] for searches producing a list of results.
 */
fun <T : Any, R> Flow<T?>.withSearching(
    debounce: Duration = DEFAULT_SEARCH_DEBOUNCE,
    search: suspend (T) -> Result<List<R>>
): Flow<SearchState<SizedList<R>>> {
    return withSizedSearching(debounce) { input -> search(input).map { it.toSizedList() } }
}

/**
 * Variant of [withSizedSearching] for searches producing a map of results.
 */
fun <T : Any, K, V> Flow<T?>.withMapSearching(
    debounce: Duration = DEFAULT_SEARCH_DEBOUNCE,
    search: suspend (T) -> Result<Map<K, V>>
): Flow<SearchState<SizedMap<K, V>>> {
    return withSizedSearching(debounce) { input -> search(input).map { it.toSizedMap() } }
}

/**
 * Variant of [withSearching] for textual queries.
 *
 * Treats any input shorter than [minQueryLength] as "no input" -> emits [SearchState.Initial]
 * without invoking [search]. The default of 1 means an empty string is the only initial case.
 */
fun <R> Flow<String>.withQuerySearching(
    debounce: Duration = DEFAULT_SEARCH_DEBOUNCE,
    minQueryLength: Int = 1,
    search: suspend (String) -> Result<List<R>>
): Flow<SearchState<SizedList<R>>> {
    return map { it.takeIf { query -> query.length >= minQueryLength } }
        .withSearching(debounce, search)
}

private fun <C : SizedContainer> Result<C>.toSearchState(): SearchState<C> {
    return fold(
        onSuccess = { it.toSearchState() },
        onFailure = { SearchState.Error(it) }
    )
}
