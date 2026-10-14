package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import org.junit.Assert.assertEquals
import org.junit.Test

class PendingRequestTrackerTest {
    @Test
    fun `no pending requests reports zero`() {
        val tracker = PendingRequestTracker()
        assertEquals(0L, tracker.update(emptySet(), nowMillis = 1_000))
    }

    @Test
    fun `age grows from when a request was first seen`() {
        val tracker = PendingRequestTracker()
        val request = Any()

        assertEquals(0L, tracker.update(setOf(request), nowMillis = 1_000))
        assertEquals(500L, tracker.update(setOf(request), nowMillis = 1_500))
    }

    @Test
    fun `reports the age of the oldest pending request`() {
        val tracker = PendingRequestTracker()
        val old = Any()
        val young = Any()

        tracker.update(setOf(old), nowMillis = 0)
        tracker.update(setOf(old, young), nowMillis = 1_000)

        assertEquals(2_000L, tracker.update(setOf(old, young), nowMillis = 2_000))
    }

    @Test
    fun `a request that is no longer pending is dropped`() {
        val tracker = PendingRequestTracker()
        val first = Any()
        val second = Any()

        tracker.update(setOf(first), nowMillis = 0)
        tracker.update(setOf(first, second), nowMillis = 1_000)

        // 'first' is gone; only 'second' (first seen at 1_000) remains.
        assertEquals(500L, tracker.update(setOf(second), nowMillis = 1_500))
    }
}
