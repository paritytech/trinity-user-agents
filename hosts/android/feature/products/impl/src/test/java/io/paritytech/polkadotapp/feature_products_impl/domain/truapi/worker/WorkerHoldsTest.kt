package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkerHoldsTest {
    private var acquired = 0
    private var released = 0
    private val holds = WorkerHolds(acquire = { acquired++ }, release = { released++ })

    @Test
    fun `each open screen of a room holds the worker until it closes`() {
        holds.openRoom("lobby")
        holds.openRoom("lobby")

        assertTrue(holds.closeRoom("lobby"))
        assertTrue(holds.isOpen("lobby"))
        assertTrue(holds.closeRoom("lobby"))

        assertEquals(2, acquired)
        assertEquals(2, released)
        assertFalse(holds.isOpen("lobby"))
    }

    @Test
    fun `closing a room that is not open releases nothing`() {
        assertFalse(holds.closeRoom("lobby"))
        holds.drop()

        assertEquals(0, released)
    }

    @Test
    fun `a failed acquire holds nothing`() {
        val failing = WorkerHolds(acquire = { error("closed") }, release = { released++ })

        runCatching { failing.openRoom("lobby") }
        failing.releaseAll()

        assertFalse(failing.isOpen("lobby"))
        assertEquals(0, released)
    }

    @Test
    fun `releasing all gives back every hold that is still held`() {
        holds.openRoom("lobby")
        holds.hold()
        holds.drop()
        holds.hold()

        holds.releaseAll()

        assertEquals(3, acquired)
        assertEquals(3, released)
        assertFalse(holds.isOpen("lobby"))
    }

    @Test
    fun `a closed chat gives back its holds and refuses a new one`() {
        holds.openRoom("lobby")

        holds.close()
        val refused = runCatching { holds.hold() }

        assertTrue(refused.isFailure)
        assertEquals(1, acquired)
        assertEquals(1, released)
    }
}
