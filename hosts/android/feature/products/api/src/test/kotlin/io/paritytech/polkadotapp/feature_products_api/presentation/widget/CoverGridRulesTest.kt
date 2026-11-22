package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import io.paritytech.polkadotapp.feature_products_api.model.JsCoverCells
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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

    private val ticket = Size(290f, 380f)
    private fun stroke(vararg xy: Float) = xy

    /** The cells a worker clears for a straight stroke: every cell whose centre lies within [reach] of the segment. */
    private fun cellsUnder(ax: Float, ay: Float, bx: Float, by: Float, reach: Float, columns: Int, rows: Int): BitSet {
        val out = BitSet()
        val width = ticket.width / columns
        val height = ticket.height / rows
        for (row in 0 until rows) for (column in 0 until columns) {
            val cx = (column + 0.5f) * width
            val cy = (row + 0.5f) * height
            if (CoverGridRules.brushCuts(cx, cy, listOf(stroke(ax, ay, bx, by)), 2 * reach)) out.set(row * columns + column)
        }
        return out
    }

    private fun inGate(gate: List<Rect>, x: Float, y: Float) = gate.any { oval ->
        val dx = x - oval.center.x
        val dy = y - oval.center.y
        dx * dx + dy * dy <= (oval.width / 2) * (oval.width / 2)
    }

    @Test
    fun `a brush stroke is a line the brush's width with round ends`() {
        val line = listOf(stroke(60f, 100f, 230f, 100f))

        assertTrue("half the width from the line is cut", CoverGridRules.brushCuts(145f, 125f, line, 52f))
        assertFalse("past half the width the cover stays", CoverGridRules.brushCuts(145f, 127f, line, 52f))
        assertTrue("the end is a half circle", CoverGridRules.brushCuts(35f, 100f, line, 52f))
        assertTrue("inside the round end", CoverGridRules.brushCuts(42f, 82f, line, 52f))
        assertFalse("a square end's corner keeps its cover", CoverGridRules.brushCuts(40f, 80f, line, 52f))
    }

    @Test
    fun `two strokes 54 dp apart leave a strip of cover between them`() {
        val strokes = listOf(stroke(35f, 55f, 255f, 55f), stroke(255f, 109f, 35f, 109f))

        assertTrue(CoverGridRules.brushCuts(145f, 80.5f, strokes, 52f) && CoverGridRules.brushCuts(145f, 83.5f, strokes, 52f))
        assertFalse("the 2 dp strip between them stays", CoverGridRules.brushCuts(145f, 82f, strokes, 52f))
    }

    @Test
    fun `a touch with no move is a dot the brush's width`() {
        val dot = listOf(stroke(100f, 100f))

        assertTrue(CoverGridRules.brushCuts(100f, 125f, dot, 52f) && CoverGridRules.brushCuts(75.2f, 100f, dot, 52f))
        assertFalse(CoverGridRules.brushCuts(100f, 127f, dot, 52f))
        assertEquals(4, CoverGridRules.brushPoints(dot).single().size)
    }

    @Test
    fun `strokes are moved from the pan's frame into the cover's`() {
        val moved = CoverGridRules.brushPoints(listOf(stroke(60f, 100f, 80f, 100f)), originX = 10f, originY = 20f).single()

        assertArrayEquals(floatArrayOf(50f, 80f, 70f, 80f), moved, 0.0001f)
    }

    @Test
    fun `the gate holds every point of a stroke whose worker cleared the cells within half the brush`() {
        val (columns, rows) = 39 to 50
        val cells = cellsUnder(40f, 70f, 250f, 160f, reach = 26f, columns = columns, rows = rows)
        val gate = CoverGridRules.brushGate(cells, columns, rows, ticket)
        val line = listOf(stroke(40f, 70f, 250f, 160f))
        var inside = 0
        var x = 0.5f
        while (x < ticket.width) {
            var y = 0.5f
            while (y < ticket.height) {
                if (CoverGridRules.brushCuts(x, y, line, 52f)) {
                    inside += 1
                    assertTrue("stroke point $x, $y outside the gate", inGate(gate, x, y))
                }
                y += 1.5f
            }
            x += 1.5f
        }
        assertTrue(inside > 1000)
    }

    @Test
    fun `the gate cuts nothing more than a cell's diagonal from a cleared cell`() {
        val (columns, rows) = 39 to 50
        val gate = CoverGridRules.brushGate(cells(10 * columns + 10), columns, rows, ticket)
        val width = ticket.width / columns
        val height = ticket.height / rows
        val diagonal = sqrt(width * width + height * height)

        assertEquals(2 * diagonal, gate.single().width, 0.001f)
        assertEquals((10.5f * width), gate.single().center.x, 0.001f)
    }

    @Test
    fun `a gate follows a right to left layout's mirrored columns`() {
        val gate = CoverGridRules.brushGate(cells(0), columns = 4, rows = 1, size = Size(40f, 10f), mirrored = true).single()

        assertEquals(35f, gate.center.x, 0.001f)
    }

    @Test
    fun `a pan's strokes start where the finger touched down, follow it, and reset when a tree arrives`() {
        val trail = JsPanTrail()
        trail.extend(3f, 4f)
        trail.extend(3f, 4f)
        trail.extend(9f, 4f)
        val before = trail.value.value.strokes
        trail.begin(20f, 20f)

        assertEquals(listOf(listOf(3f, 4f, 9f, 4f), listOf(20f, 20f)), trail.value.value.strokes.map { it.toList() })
        assertEquals("a published value is never changed", 1, before.size)

        trail.reset()
        assertTrue(trail.value.value.strokes.isEmpty())
    }
}
