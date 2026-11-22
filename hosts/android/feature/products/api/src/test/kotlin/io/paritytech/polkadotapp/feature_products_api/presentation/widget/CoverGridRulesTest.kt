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
    fun `the edge is softened by half the cell's shorter side, and not at all without a grid`() {
        val ticket = Size(272f, 340f)

        assertEquals(4f, CoverGridRules.softening(34, 42, ticket), 0.001f)
        assertEquals(2f, CoverGridRules.softening(68, 85, ticket), 0.001f)
        assertEquals(0f, CoverGridRules.softening(0, 42, ticket), 0f)
        assertEquals(0f, CoverGridRules.softening(34, 42, Size.Zero), 0f)
    }

    @Test
    fun `the blur radius gives the softening as the blur's spread, as Android converts it`() {
        val radius = CoverGridRules.blurRadius(10.5f)

        assertEquals(10.5f, 0.57735f * radius + 0.5f, 0.001f)
        assertEquals(0f, CoverGridRules.blurRadius(0.2f), 0f)
    }

    @Test
    fun `sharpening keeps the half-clear edge where it is and makes the rest clear or covered`() {
        val matrix = CoverGridRules.sharpenEdge()
        val opacity = { alpha: Float -> (matrix[18] * alpha * 255f + matrix[19]).coerceIn(0f, 255f) / 255f }
        val step = 1f / CoverGridRules.EDGE_GAIN

        assertEquals(0.5f, opacity(0.5f), 0.001f)
        assertEquals(0f, opacity(0.5f - step), 0f)
        assertEquals(1f, opacity(0.5f + step), 0f)
        assertEquals(listOf(1f, 1f, 1f), listOf(matrix[0], matrix[6], matrix[12]))
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
