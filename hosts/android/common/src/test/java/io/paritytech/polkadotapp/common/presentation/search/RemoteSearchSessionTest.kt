package io.paritytech.polkadotapp.common.presentation.search

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

class RemoteSearchSessionTest {
    @Test
    fun `a fast response replaces the pending phase without a loader`() = runBlocking<Unit> {
        val session = session(responseDelay = Duration.ZERO) { Result.success(listOf("tea")) }

        val phases = session.phases("te").toList()

        assertEquals(
            listOf(
                RemoteSearchPhase.Pending(emptyList(), loaderDue = false),
                RemoteSearchPhase.Loaded(listOf("tea")),
            ),
            phases
        )
    }

    @Test
    fun `a late response lets the loader become due first`() = runBlocking<Unit> {
        val session = session(responseDelay = LOADER_DELAY * 3) { Result.success(listOf("tea")) }

        val phases = session.phases("te").toList()

        assertEquals(
            listOf(
                RemoteSearchPhase.Pending(emptyList(), loaderDue = false),
                RemoteSearchPhase.Pending(emptyList(), loaderDue = true),
                RemoteSearchPhase.Loaded(listOf("tea")),
            ),
            phases
        )
    }

    @Test
    fun `a longer query keeps the last response filtered while it is pending`() = runBlocking<Unit> {
        val session = session(responseDelay = Duration.ZERO) { query ->
            Result.success(listOf("tea", "tent", "test").filter { it.startsWith(query) })
        }

        session.phases("te").toList()
        val phases = session.phases("tes").toList()

        assertEquals(RemoteSearchPhase.Pending(listOf("test"), loaderDue = false), phases.first())
    }

    @Test
    fun `a shorter query keeps the last response while it is pending`() = runBlocking<Unit> {
        val session = session(responseDelay = Duration.ZERO) { query ->
            Result.success(listOf("tea", "tent", "test").filter { it.startsWith(query) })
        }

        session.phases("tes").toList()
        val phases = session.phases("te").toList()

        assertEquals(RemoteSearchPhase.Pending(listOf("test"), loaderDue = false), phases.first())
    }

    @Test
    fun `an unrelated query drops the last response`() = runBlocking<Unit> {
        val session = session(responseDelay = Duration.ZERO) { Result.success(listOf("tea")) }

        session.phases("te").toList()
        val phases = session.phases("ab").toList()

        assertEquals(RemoteSearchPhase.Pending(emptyList<String>(), loaderDue = false), phases.first())
    }

    @Test
    fun `a failed lookup reports failure and forgets the last response`() = runBlocking<Unit> {
        var fail = false
        val session = session(responseDelay = Duration.ZERO) {
            if (fail) Result.failure(IllegalStateException()) else Result.success(listOf("tea"))
        }

        session.phases("te").toList()
        fail = true
        val failedPhases = session.phases("tea").toList()
        fail = false
        val afterFailure = session.phases("tea").toList()

        assertEquals(RemoteSearchPhase.Failed, failedPhases.last())
        assertEquals(RemoteSearchPhase.Pending(emptyList<String>(), loaderDue = false), afterFailure.first())
    }

    private fun session(
        responseDelay: Duration,
        search: (String) -> Result<List<String>>,
    ) = RemoteSearchSession(
        search = { query ->
            delay(responseDelay)
            search(query)
        },
        matchesQuery = { result, query -> result.startsWith(query, ignoreCase = true) },
        debounce = DEBOUNCE,
        loaderDelay = LOADER_DELAY,
    )

    private companion object {
        val DEBOUNCE = 5.milliseconds
        val LOADER_DELAY = 60.milliseconds
    }
}
