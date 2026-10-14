package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * One wait for a group's inputs decides what a policy may build and what it may give up on. Returning too early
 * builds half a group or gives up on an input still landing; returning too late holds a payment for nothing.
 */
@OptIn(ExperimentalTime::class)
class AwaitInputsTest {
    private val looks = Channel<Set<String>>(Channel.UNLIMITED)

    @Test
    fun `returns as soon as every input is present`() = runTest {
        look(setOf(A, B))

        val result = awaitAB(deadline = at(10.minutes))

        assertEquals(setOf(A, B), result.present)
        assertEquals(0L, currentTime)
    }

    @Test
    fun `holds out for the rest once some inputs are present, then settles for the latest look`() = runTest {
        look(setOf(A))

        val result = awaitAB(deadline = at(10.minutes))

        assertEquals(setOf(A), result.present)
        assertEquals(30.seconds.inWholeMilliseconds, currentTime)
    }

    @Test
    fun `a look completing the set ends the hold-out early`() = runTest {
        look(setOf(A))
        lookAfter(5.seconds, setOf(A, B))

        val result = awaitAB(deadline = at(10.minutes))

        assertEquals(setOf(A, B), result.present)
        assertEquals(5.seconds.inWholeMilliseconds, currentTime)
    }

    /** A fork can take an input away; building against the widest view ever seen would spend what is gone. */
    @Test
    fun `the newest look wins over a wider earlier one`() = runTest {
        look(setOf(A))
        lookAfter(5.seconds, emptySet())

        val result = awaitAB(deadline = at(10.minutes))

        assertEquals(emptySet<String>(), result.present)
    }

    @Test
    fun `inputs nobody asked about are ignored`() = runTest {
        look(setOf(A, B, "other"))

        val result = awaitAB(deadline = at(10.minutes))

        assertEquals(setOf(A, B), result.present)
    }

    @Test
    fun `nothing present keeps waiting while the deadline is ahead`() = runTest {
        look(emptySet())
        lookAfter(1.minutes, setOf(A, B))

        val result = awaitAB(deadline = at(10.minutes))

        assertEquals(setOf(A, B), result.present)
        assertEquals(1.minutes.inWholeMilliseconds, currentTime)
    }

    @Test
    fun `nothing present returns once the deadline has passed`() = runTest {
        look(emptySet())

        val result = awaitAB(deadline = at(Duration.ZERO))

        assertTrue(result.looked)
        assertEquals(emptySet<String>(), result.present)
    }

    /** A quiet chain emits nothing new, so waiting for the next look alone would miss the deadline for minutes. */
    @Test
    fun `a deadline passing on a quiet chain is noticed without a new look`() = runTest {
        look(emptySet())

        val result = awaitAB(deadline = at(20.seconds))

        assertTrue(result.looked)
        assertEquals(20.seconds.inWholeMilliseconds, currentTime)
    }

    @Test
    fun `a look that never arrives gives up at the idle limit without having looked`() = runTest {
        val result = awaitAB(deadline = at(Duration.ZERO))

        assertFalse(result.looked)
        assertEquals(5.minutes.inWholeMilliseconds, currentTime)
    }

    @Test
    fun `waiting on nothing present ends at the idle limit even before the deadline`() = runTest {
        look(emptySet())

        val result = awaitAB(deadline = at(1_000.minutes))

        assertTrue(result.looked)
        assertEquals(emptySet<String>(), result.present)
        assertEquals(5.minutes.inWholeMilliseconds, currentTime)
    }

    // ---- InputsLook ----

    @Test
    fun `an input seen missing past its deadline is abandoned`() {
        val look = InputsLook(present = emptySet<String>(), looked = true, takenAt = at(1.minutes))

        assertTrue(look.abandoned(A, deadline = at(Duration.ZERO)))
    }

    @Test
    fun `an input seen missing before its deadline is not abandoned`() {
        val look = InputsLook(present = emptySet<String>(), looked = true, takenAt = at(Duration.ZERO))

        assertFalse(look.abandoned(A, deadline = at(1.minutes)))
    }

    @Test
    fun `a present input is never abandoned`() {
        val look = InputsLook(present = setOf(A), looked = true, takenAt = at(1.minutes))

        assertFalse(look.abandoned(A, deadline = at(Duration.ZERO)))
    }

    /** Only a look can say an input is absent; no look at all proves nothing. */
    @Test
    fun `nothing is abandoned without a look`() {
        val look = InputsLook(present = emptySet<String>(), looked = false, takenAt = at(1.minutes))

        assertFalse(look.abandoned(A, deadline = at(Duration.ZERO)))
    }

    // ---- harness ----

    private suspend fun TestScope.awaitAB(deadline: Instant): InputsLook<String> =
        awaitInputs(presence(), wanted = setOf(A, B), deadline = deadline, timeProvider = virtualClock())

    private fun presence(): Flow<Set<String>> = looks.receiveAsFlow()

    private fun look(present: Set<String>) {
        looks.trySend(present)
    }

    private fun TestScope.lookAfter(after: Duration, present: Set<String>) {
        backgroundScope.launch {
            delay(after)
            looks.send(present)
        }
    }

    /** The deadline and the waits run on the same clock, so a test reads in one timeline. */
    private fun TestScope.virtualClock(): TimeProvider = mockk<TimeProvider>().also {
        every { it.now() } answers { at(currentTime.milliseconds) }
    }

    private fun at(offset: Duration): Instant = BASE + offset

    private companion object {
        const val A = "a"
        const val B = "b"

        val BASE: Instant = Instant.fromEpochSeconds(1_000_000)
    }
}
