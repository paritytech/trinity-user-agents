package io.paritytech.polkadotapp.feature_products_api.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class JsPanTrailTest {
    @Test
    fun `the strokes the draw reads follow every begin, extend and reset`() {
        val trail = JsPanTrail()
        trail.begin(1f, 2f)
        trail.extend(3f, 4f)
        assertEquals(trail.value.value, trail.drawn)
        assertEquals(listOf(1f, 2f, 3f, 4f), trail.drawn.strokes.single().toList())

        trail.extend(3f, 4f)
        assertEquals(2L, trail.drawn.version)

        trail.begin(5f, 6f)
        assertEquals(trail.value.value, trail.drawn)
        assertEquals(2, trail.drawn.strokes.size)

        trail.reset()
        assertEquals(trail.value.value, trail.drawn)
        assertTrue(trail.drawn.strokes.isEmpty())
        assertEquals(4L, trail.drawn.version)
    }

    @Test
    fun `a move with no stroke begun starts one`() {
        val trail = JsPanTrail()
        trail.extend(7f, 8f)

        assertEquals(listOf(7f, 8f), trail.drawn.strokes.single().toList())
    }
}
