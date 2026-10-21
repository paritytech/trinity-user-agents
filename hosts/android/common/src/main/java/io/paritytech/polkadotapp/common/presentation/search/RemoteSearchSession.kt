package io.paritytech.polkadotapp.common.presentation.search

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.launch
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

sealed interface RemoteSearchPhase<out T> {
    val results: List<T>

    data class Pending<T>(override val results: List<T>, val loaderDue: Boolean) : RemoteSearchPhase<T>

    data class Loaded<T>(override val results: List<T>) : RemoteSearchPhase<T>

    data object Failed : RemoteSearchPhase<Nothing> {
        override val results: List<Nothing> = emptyList()
    }
}

class RemoteSearchSession<T>(
    private val search: suspend (String) -> Result<List<T>>,
    private val matchesQuery: (T, String) -> Boolean,
    private val debounce: Duration = 300.milliseconds,
    private val loaderDelay: Duration = 800.milliseconds,
) {
    @Volatile
    private var lastResponse: Response<T>? = null

    fun phases(query: String): Flow<RemoteSearchPhase<T>> = channelFlow {
        val preserved = preservedResults(query)
        send(RemoteSearchPhase.Pending(preserved, loaderDue = false))

        val loader = launch {
            delay(loaderDelay)
            send(RemoteSearchPhase.Pending(preserved, loaderDue = true))
        }

        delay(debounce)
        val result = search(query)
        currentCoroutineContext().ensureActive()
        loader.cancel()

        result
            .onSuccess { results ->
                lastResponse = Response(query, results)
                send(RemoteSearchPhase.Loaded(results))
            }
            .onFailure {
                lastResponse = null
                send(RemoteSearchPhase.Failed)
            }
    }

    private fun preservedResults(query: String): List<T> {
        val response = lastResponse ?: return emptyList()
        val related = query.startsWith(response.query, ignoreCase = true) ||
            response.query.startsWith(query, ignoreCase = true)

        return if (related) response.results.filter { matchesQuery(it, query) } else emptyList()
    }

    private class Response<T>(val query: String, val results: List<T>)
}
