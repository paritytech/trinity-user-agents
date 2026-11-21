package io.paritytech.polkadotapp.feature_products_api.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.concurrent.thread

class JsCoverCellsTest {
    @Test
    fun `the value the draw reads follows every clear and reset`() {
        val cover = JsCoverCells()
        cover.clear(listOf(3, 4))
        assertEquals(cover.value.value, cover.drawn)
        assertEquals(2, cover.drawn.cells.cardinality())

        cover.clear(listOf(4, 9, -1, JsCoverCells.MAX_CELLS))
        assertEquals(cover.value.value, cover.drawn)
        assertEquals(3, cover.drawn.cells.cardinality())

        cover.reset()
        assertEquals(cover.value.value, cover.drawn)
        assertTrue(cover.drawn.cells.isEmpty)
        assertEquals(3L, cover.drawn.version)
    }

    @Test
    fun `cells cleared on the render stream's thread can be read at once by the draw`() {
        val cover = JsCoverCells()
        thread { cover.clear(listOf(1, 2, 3)) }.join()

        assertEquals(3, cover.drawn.cells.cardinality())
    }
}
