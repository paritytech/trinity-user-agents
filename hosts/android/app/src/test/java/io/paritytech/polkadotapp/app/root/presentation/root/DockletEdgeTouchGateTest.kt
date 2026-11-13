package io.paritytech.polkadotapp.app.root.presentation.root

import org.junit.Assert.assertEquals
import org.junit.Test

class DockletEdgeTouchGateTest {
    private val gate = DockletEdgeTouchGate(touchSlop = Slop)

    @Test
    fun `a touch away from the Back edges counts at once`() {
        assertEquals(Inside, gate.onDown(Inside, onBackGestureEdge = false))
        assertEquals(null, gate.onUp())
    }

    @Test
    fun `a Back swipe the system takes never counts`() {
        assertEquals(null, gate.onDown(Edge, onBackGestureEdge = true))
        assertEquals(null, gate.onMove(Edge.x + 40f, Edge.y + Slop + 4f))
        gate.onCancel()

        assertEquals(null, gate.onUp())
    }

    @Test
    fun `a tap on a Back edge counts when the finger lifts`() {
        gate.onDown(Edge, onBackGestureEdge = true)

        assertEquals(Edge, gate.onUp())
        assertEquals(null, gate.onUp())
    }

    @Test
    fun `a scroll from a Back edge counts once it moves steeply past the slop`() {
        gate.onDown(Edge, onBackGestureEdge = true)

        assertEquals(null, gate.onMove(Edge.x + 2f, Edge.y + Slop))
        assertEquals(Edge, gate.onMove(Edge.x + 2f, Edge.y + Slop + 1f))
        assertEquals(null, gate.onUp())
    }

    private companion object {
        const val Slop = 8f
        val Inside = DockletInput.Touch(540f, 1200f)
        val Edge = DockletInput.Touch(4f, 1200f)
    }
}
