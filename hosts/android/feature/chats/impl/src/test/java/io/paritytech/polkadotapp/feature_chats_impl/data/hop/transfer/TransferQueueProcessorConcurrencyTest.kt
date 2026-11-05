@file:OptIn(ExperimentalCoroutinesApi::class)

package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.thenSuspend
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

class TransferQueueProcessorConcurrencyTest {
    @Test
    fun `three pending uploads are all in flight when ten slots are free`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", "b", "c")

        startPass(queue)
        runCurrent()

        assertEquals(3, queue.inFlightCount)
        assertNoSpin(queue)
    }

    @Test
    fun `ten uploads are in flight and the eleventh waits when eleven are pending`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = itemIds(11)
        queue.addHanging(*ids.toTypedArray())

        startPass(queue)
        runCurrent()

        assertEquals(MAX_IN_FLIGHT, queue.inFlightCount)
        assertNotStarted(queue, ids.last())
    }

    @Test
    fun `the eleventh upload starts when one of ten in flight completes`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = itemIds(11)
        queue.addHanging(*ids.toTypedArray())

        startPass(queue)
        runCurrent()
        assertEquals(MAX_IN_FLIGHT, queue.inFlightCount)

        queue.complete(ids.first())
        runCurrent()

        assertStarted(queue, ids.last())
        assertEquals(MAX_IN_FLIGHT, queue.inFlightCount)
        assertEquals(RowStatus.DONE, queue.statusOf(ids.first()))
    }

    @Test
    fun `a pending upload reaches done when an earlier upload's attempt never returns`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", "b")

        startPass(queue)
        runCurrent()
        queue.complete("b")
        runCurrent()

        assertEquals(RowStatus.DONE, queue.statusOf("b"))
        assertStillInFlight(queue, "a")
    }

    @Test
    fun `an upload reaches done when it is queued after a stuck attempt started`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a")

        startPass(queue)
        runCurrent()

        queue.addHanging("c")
        queue.emitChange()
        runCurrent()
        queue.complete("c")
        runCurrent()

        assertStarted(queue, "c")
        assertEquals(RowStatus.DONE, queue.statusOf("c"))
        assertStillInFlight(queue, "a")
    }

    @Test
    fun `an upload queued while all ten slots are busy starts when a slot frees`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = itemIds(10)
        queue.addHanging(*ids.toTypedArray())

        startPass(queue)
        runCurrent()

        queue.addHanging("late")
        queue.emitChange()
        runCurrent()
        assertNotStarted(queue, "late")

        queue.complete(ids.first())
        runCurrent()

        assertStarted(queue, "late")
    }

    @Test
    fun `an upload starts at its due time when its backoff ends while another attempt is in flight`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a")
        queue.addHanging("b", nextAttemptAt = BACKOFF_DUE)

        startPass(queue)
        runCurrent()

        advanceTimeBy(BACKOFF_DUE - DUE_MARGIN)
        runCurrent()
        assertNotStarted(queue, "b")

        advanceTimeBy(DUE_MARGIN * 2)
        runCurrent()

        assertStarted(queue, "b")
        assertStillInFlight(queue, "a")
    }

    @Test
    fun `the pass does not re-query in a loop when a retried upload in flight has a past nextAttemptAt`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", nextAttemptAt = (-10).seconds, retryState = retriedOnce())
        queue.addHanging("b", nextAttemptAt = 1.hours)

        startPass(queue)
        runCurrent()
        advanceTimeBy(LONG_WAIT)
        runCurrent()

        assertTrue("claim calls: ${queue.claimCalls}", queue.claimCalls <= CLAIM_CALL_BOUND)
        assertNoSpin(queue)
        assertStillInFlight(queue, "a")
        assertNotStarted(queue, "b")
    }

    @Test
    fun `the slot is freed when a completion and a backoff end at the same instant`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addDelayed("a", BACKOFF_DUE)
        queue.addHanging("b", nextAttemptAt = BACKOFF_DUE)

        val pass = startPass(queue)
        runCurrent()
        advanceTimeBy(BACKOFF_DUE)
        runCurrent()

        assertEquals(queue.returnedAt("a"), queue.firstClaimCallAtOrAfter(BACKOFF_DUE))
        assertEquals(RowStatus.DONE, queue.statusOf("a"))
        assertEquals(1, queue.inFlightCount)
        assertStarted(queue, "b")
        assertNoSpin(queue)

        queue.complete("b")
        runCurrent()

        assertTrue(pass.isCompleted)
    }

    @Test
    fun `a second upload starts when it is queued later in the same pass`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a")

        startPass(queue)
        runCurrent()

        queue.addHanging("c")
        queue.emitChange()
        runCurrent()
        assertStarted(queue, "c")
        queue.complete("c")
        runCurrent()

        queue.addHanging("d")
        queue.emitChange()
        runCurrent()

        assertStarted(queue, "d")
    }

    @Test
    fun `each upload is in flight once when two are pending and ten slots are free`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", "b")

        startPass(queue)
        runCurrent()
        queue.emitChange()
        runCurrent()

        assertEquals(listOf("a", "b"), queue.startedIds.sorted())
        assertNoSpin(queue)
    }

    @Test
    fun `an upload left in progress by a killed process is attempted when the pass starts`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", status = RowStatus.IN_PROGRESS)

        startPass(queue)
        runCurrent()

        assertEquals(listOf("a"), queue.startedIds)
    }

    @Test
    fun `an upload keeps its window anchor when its third attempt fails`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val retriedTwice = retriedTwice()
        queue.addHanging("a", retryState = retriedTwice)

        failAttemptOf(queue, "a")

        assertEquals(retriedTwice.firstFailureAt, queue.retryStateOf("a").firstFailureAt)
    }

    @Test
    fun `an upload backs off forty seconds when its third attempt fails`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", retryState = retriedTwice())

        failAttemptOf(queue, "a")

        assertEquals(virtualNow() + 40.seconds, queue.retryStateOf("a").nextAttemptAt)
    }

    @Test
    fun `the eleventh upload starts when one of ten in flight fails`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = withPendingUploads(queue, count = 11)

        startPass(queue)
        runCurrent()
        assertNotStarted(queue, ids.last())
        queue.fail(ids.first(), RetryableTransferException("timeout"))
        runCurrent()

        assertStarted(queue, ids.last())
    }

    @Test
    fun `the eleventh upload starts when one of ten in flight is cancelled`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = itemIds(11)
        queue.addHanging(*ids.toTypedArray())

        startPass(queue)
        runCurrent()
        queue.cancel(ids.first())
        runCurrent()

        assertStarted(queue, ids.last())
        assertEquals(1, queue.startedIds.count { it == ids.first() })
    }

    @Test
    fun `the eleventh upload starts when one of ten in flight fails terminally`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = withPendingUploads(queue, count = 11)

        startPass(queue)
        runCurrent()
        assertNotStarted(queue, ids.last())
        queue.fail(ids.first(), TerminalTransferException("rejected"))
        runCurrent()

        assertStarted(queue, ids.last())
        assertEquals(RowStatus.FAILED, queue.statusOf(ids.first()))
    }

    @Test
    fun `a due upload starts when it became due between the claim and the backoff query`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.backoffQueryLatency = BACKOFF_QUERY_LATENCY
        queue.addHanging("a")
        // The wake from a's start cuts the first wait short, so the second backoff query is the one that ends at 2x latency.
        val dueWhenSecondBackoffQueryEnds = BACKOFF_QUERY_LATENCY * 2
        queue.addHanging("r", nextAttemptAt = dueWhenSecondBackoffQueryEnds)

        startPass(queue)
        runCurrent()
        advanceTimeBy(dueWhenSecondBackoffQueryEnds)
        runCurrent()

        assertStarted(queue, "r")
        assertStillInFlight(queue, "a")
    }

    @Test
    fun `the pass waits for a slot without re-querying when every slot is busy and a due upload is pending`() = runTest {
        val queue = FakeConcurrentQueue(this)
        withPendingUploads(queue, count = MAX_IN_FLIGHT)
        queue.addHanging("due", nextAttemptAt = (-10).seconds)

        startPass(queue)
        runCurrent()
        advanceTimeBy(LONG_WAIT)
        runCurrent()

        assertNotStarted(queue, "due")
        assertNoQueriesOnceFull(queue)
    }

    @Test
    fun `a due upload starts when a slot frees while every slot was busy`() = runTest {
        val queue = FakeConcurrentQueue(this)
        val ids = withPendingUploads(queue, count = MAX_IN_FLIGHT)
        queue.addHanging("due", nextAttemptAt = (-10).seconds)

        startPass(queue)
        runCurrent()
        queue.complete(ids.first())
        runCurrent()

        assertStarted(queue, "due")
    }

    @Test
    fun `the pass returns at once when nothing is pending`() = runTest {
        val queue = FakeConcurrentQueue(this)

        val pass = startPass(queue)
        runCurrent()

        assertTrue(pass.isCompleted)
    }

    @Test
    fun `the pass returns when the only pending upload is waiting out its backoff`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", nextAttemptAt = 1.hours)

        val pass = startPass(queue)
        runCurrent()

        assertTrue(pass.isCompleted)
        assertNotStarted(queue, "a")
    }

    @Test
    fun `the pass does not return when an attempt is still in flight`() = runTest {
        val queue = FakeConcurrentQueue(this)
        queue.addHanging("a", "b")

        val pass = startPass(queue)
        runCurrent()
        assertTrue(pass.isActive)

        queue.complete("a")
        queue.complete("b")
        runCurrent()

        assertTrue(pass.isCompleted)
    }

    @Test
    fun `the sequential pass starts one item at a time when three are pending`() = runTest {
        val queue = sequentialQueueWithHanging("a", "b", "c")

        backgroundScope.launch { processor().processPending(queue) }
        runCurrent()

        verifyProcessedOnce(queue)
    }

    private fun TestScope.processor() =
        TransferQueueProcessor(HopTransferRetryPolicy(), FakeTimeProvider { testScheduler.currentTime })

    private fun TestScope.startPass(queue: FakeConcurrentQueue): Job {
        val processor = processor()
        return backgroundScope.launch { processor.processPendingConcurrently(queue) }
    }

    private fun withPendingUploads(queue: FakeConcurrentQueue, count: Int): List<String> {
        val ids = itemIds(count)
        queue.addHanging(*ids.toTypedArray())
        return ids
    }

    private fun itemIds(count: Int) = List(count) { "item-$it" }

    private fun TestScope.virtualNow(): Instant = Instant.fromEpochMilliseconds(testScheduler.currentTime)

    private fun TestScope.retriedOnce() =
        HopTransferRetryState(attemptCount = 1, firstFailureAt = virtualNow() - 1.minutes, nextAttemptAt = null)

    private fun TestScope.retriedTwice() =
        HopTransferRetryState(attemptCount = 2, firstFailureAt = virtualNow() - 1.hours, nextAttemptAt = null)

    private fun TestScope.failAttemptOf(queue: FakeConcurrentQueue, id: String) {
        startPass(queue)
        runCurrent()
        queue.fail(id, RetryableTransferException("timeout"))
        runCurrent()
    }

    private suspend fun sequentialQueueWithHanging(vararg ids: String): TransferQueue<FakeItem> {
        @Suppress("UNCHECKED_CAST")
        val queue = mock(TransferQueue::class.java) as TransferQueue<FakeItem>
        val items = ids.map { FakeItem(it, HopTransferRetryState.None) }
        whenever(queue.nextPending()).thenReturn(items.first(), *(items.drop(1) + null).toTypedArray())
        whenever(queue.process(any())).thenSuspend { awaitCancellation() }
        return queue
    }

    private suspend fun verifyProcessedOnce(queue: TransferQueue<FakeItem>) {
        verify(queue, times(1)).process(any())
    }

    private fun assertStarted(queue: FakeConcurrentQueue, id: String) {
        assertTrue("$id should have started; started: ${queue.startedIds}", id in queue.startedIds)
    }

    private fun assertNotStarted(queue: FakeConcurrentQueue, id: String) {
        assertFalse("$id should not have started; started: ${queue.startedIds}", id in queue.startedIds)
    }

    private fun assertStillInFlight(queue: FakeConcurrentQueue, id: String) {
        assertTrue("$id should still be in flight", queue.isInFlight(id))
    }

    private fun assertNoQueriesOnceFull(queue: FakeConcurrentQueue) {
        assertEquals("one claim per slot filled", MAX_IN_FLIGHT, queue.claimCalls)
        assertEquals("no backoff query while every slot is busy", 0, queue.backoffQueryCalls)
    }

    private fun assertNoSpin(queue: FakeConcurrentQueue) {
        assertFalse("the pass re-queried in a loop", queue.spinDetected)
    }

    private companion object {
        const val CLAIM_CALL_BOUND = 5
        val BACKOFF_DUE = 30.seconds
        val DUE_MARGIN = 1.seconds
        val LONG_WAIT = 30.minutes
        val BACKOFF_QUERY_LATENCY = 1.milliseconds
    }
}
