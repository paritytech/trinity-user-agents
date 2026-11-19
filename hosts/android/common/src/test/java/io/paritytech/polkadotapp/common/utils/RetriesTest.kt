package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class RetriesTest {
    @Test
    fun `waits through awaitRetry with the failure and the planned delay before trying again`() = runBlocking<Unit> {
        val failure = IllegalStateException()
        val attempts = ArrayDeque(listOf(Result.failure(failure), Result.success(7)))
        val waits = mutableListOf<Pair<Throwable, Duration>>()

        val result = retryUntilSuccess(
            retryDelay = { attempt -> attempt.seconds },
            awaitRetry = { error, retryIn -> waits += error to retryIn },
        ) { attempts.removeFirst() }

        assertEquals(7, result)
        assertEquals(listOf<Pair<Throwable, Duration>>(failure to 1.seconds), waits)
    }
}
