package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.mockk
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.NativeWorkerClaim

class WorkerHoldsTest {
    private val claims = mutableListOf<NativeWorkerClaim>()
    private val released = mutableListOf<NativeWorkerClaim>()
    private val holds = WorkerHolds(
        claim = { mockk<NativeWorkerClaim>(relaxed = true).also { claims += it } },
        release = { released += it },
    )

    @Test
    fun `each open screen of a room holds the worker until it closes`() {
        holds.openRoom("lobby")
        holds.openRoom("lobby")

        assertTrue(holds.closeRoom("lobby"))
        assertTrue(holds.isOpen("lobby"))
        assertTrue(holds.closeRoom("lobby"))

        assertEquals(2, claims.size)
        assertEveryClaimReleasedOnce()
        assertFalse(holds.isOpen("lobby"))
    }

    @Test
    fun `closing a room that is not open releases nothing`() {
        assertFalse(holds.closeRoom("lobby"))
        holds.drop()

        assertEquals(emptyList<NativeWorkerClaim>(), released)
    }

    @Test
    fun `a failed claim holds nothing`() {
        val failing = WorkerHolds(claim = { error("closed") }, release = { released += it })

        runCatching { failing.openRoom("lobby") }
        failing.releaseAll()

        assertFalse(failing.isOpen("lobby"))
        assertEquals(emptyList<NativeWorkerClaim>(), released)
    }

    @Test
    fun `releasing all gives back every hold that is still held`() {
        holds.openRoom("lobby")
        holds.hold()
        holds.drop()
        holds.hold()

        holds.releaseAll()

        assertEquals(3, claims.size)
        assertEveryClaimReleasedOnce()
        assertFalse(holds.isOpen("lobby"))
    }

    @Test
    fun `a closed chat gives back its holds and refuses a new one`() {
        holds.openRoom("lobby")

        holds.close()
        val refused = runCatching { holds.hold() }

        assertTrue(refused.isFailure)
        assertEquals(1, claims.size)
        assertEveryClaimReleasedOnce()
    }

    private fun assertEveryClaimReleasedOnce() {
        assertEquals(claims.size, released.size)
        assertEquals(claims.toSet(), released.toSet())
    }
}
