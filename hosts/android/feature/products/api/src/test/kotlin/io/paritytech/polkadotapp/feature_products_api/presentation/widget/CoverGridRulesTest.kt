package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Size
import io.paritytech.polkadotapp.feature_products_api.model.JsCoverCells
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.BitSet
import kotlin.math.sqrt

class CoverGridRulesTest {
    private fun cells(vararg cell: Int) = BitSet().apply { cell.forEach(::set) }

    @Test
    fun `each cleared cell is a hole centred on it that reaches its corners`() {
        // Row 1, column 1 of a 4 x 2 grid of 10 x 10 cells.
        val hole = CoverGridRules.holes(cells(5), columns = 4, rows = 2, size = Size(40f, 20f)).single()
        val radius = sqrt(50f)

        assertEquals(15f, hole.center.x, 0.001f)
        assertEquals(15f, hole.center.y, 0.001f)
        assertEquals(2 * radius, hole.width, 0.001f)
        assertTrue("the cell's corner is inside its hole", (10.2f - 15f).let { it * it * 2 } <= radius * radius)
    }

    @Test
    fun `neighbouring holes overlap so no cover is left between them`() {
        val (left, right) = CoverGridRules.holes(cells(0, 1), columns = 2, rows = 1, size = Size(20f, 10f))

        assertTrue(left.right > right.left)
    }

    @Test
    fun `right to left puts the first column at the right edge`() {
        val hole = CoverGridRules.holes(cells(0), columns = 4, rows = 1, size = Size(40f, 10f), mirrored = true).single()

        assertEquals(35f, hole.center.x, 0.001f)
    }

    @Test
    fun `cells past the grid draw nothing`() {
        assertEquals(emptyList<Any>(), CoverGridRules.holes(cells(8), columns = 4, rows = 2, size = Size(40f, 20f)))
    }

    @Test
    fun `a finger's place is kept inside the node and measured from its start edge`() {
        assertEquals(40f to 50f, GestureRules.inside(40f, 50f, widthPx = 280, heightPx = 300, rtl = false))
        assertEquals(0f to 300f, GestureRules.inside(-9f, 400f, widthPx = 280, heightPx = 300, rtl = false))
        assertEquals(240f to 50f, GestureRules.inside(40f, 50f, widthPx = 280, heightPx = 300, rtl = true))
    }

    @Test
    fun `worker cells add up, ignore cells outside any grid, and reset when a tree arrives`() {
        val cover = JsCoverCells()
        cover.clear(listOf(3, 3, 9, -1, JsCoverCells.MAX_CELLS))
        assertEquals(listOf(3, 9), cover.value.value.cells.stream().toArray().toList())
        val before = cover.value.value.cells

        cover.clear(listOf(10))
        assertEquals("a published value is never changed", 2, before.cardinality())

        cover.reset()
        assertTrue(cover.value.value.cells.isEmpty)
    }
}
