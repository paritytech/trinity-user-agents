package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrush
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrushTip
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MaskStampsTest {
    private val ellipse = JsMaskBrush(JsMaskBrushTip.ELLIPSE, 500, 0, 500)

    @Test
    fun `spacing carries across corners and includes a bare last point`() {
        val points = listOf(JsMaskPoint(0, 0), JsMaskPoint(300, 0), JsMaskPoint(300, 600))
        assertEquals(
            listOf(MaskStamps.Centre(0.0, 0.0), MaskStamps.Centre(300.0, 200.0), MaskStamps.Centre(300.0, 600.0)),
            MaskStamps.centres(1000, ellipse, points),
        )
    }

    @Test
    fun `repeated points and zero width do not add invalid stamps`() {
        val points = listOf(JsMaskPoint(100, 200), JsMaskPoint(100, 200))
        assertEquals(listOf(MaskStamps.Centre(100.0, 200.0)), MaskStamps.centres(1000, ellipse, points))
        assertTrue(MaskStamps.centres(0, ellipse, points).isEmpty())
        assertTrue(MaskStamps.centres(1000, ellipse, emptyList()).isEmpty())
    }

    @Test
    fun `spacing and aspect use the core clamps`() {
        val points = listOf(JsMaskPoint(0, 0), JsMaskPoint(100, 0))
        assertEquals(3, MaskStamps.centres(1000, ellipse.copy(spacingPermille = 0), points).size)
        assertEquals(2, MaskStamps.centres(1000, ellipse.copy(spacingPermille = 65000), points).size)
        val origin = MaskStamps.Centre(0.0, 0.0)
        assertTrue(MaskStamps.covers(1000, ellipse.copy(aspectPermille = 65000), origin, 0.0, 500.0))
        assertFalse(MaskStamps.covers(1000, ellipse.copy(aspectPermille = 0), origin, 0.0, 1.0))
    }

    @Test
    fun `a stamped line has at most the core's most stamps, never closer than half a dp, from end to end`() {
        val thin = ellipse.copy(spacingPermille = 50)
        assertEquals(MaskStamps.MAX_STAMPS, MaskStamps.centres(1, thin, listOf(JsMaskPoint(0, 0), JsMaskPoint(10_000_000, 0))).size)
        val widest = listOf(JsMaskPoint(Int.MIN_VALUE, 0), JsMaskPoint(Int.MAX_VALUE, 0))
        val stamps = MaskStamps.centres(1, thin, widest)
        assertEquals(MaskStamps.MAX_STAMPS, stamps.size)
        assertEquals(MaskStamps.Centre(Int.MIN_VALUE.toDouble(), 0.0), stamps.first())
        assertEquals("the last stamp is on the last point", Int.MAX_VALUE.toDouble(), stamps.last().x, 1.0)
        assertEquals("every half a dp", 5, MaskStamps.centres(1, thin, listOf(JsMaskPoint(0, 0), JsMaskPoint(200, 0))).size)
        val zigzag = (0 until 10_000).map { JsMaskPoint(if (it % 2 == 0) 0 else 100_000, it) }
        assertTrue(MaskStamps.centres(1, thin, zigzag).size <= MaskStamps.MAX_STAMPS)
    }

    @Test
    fun `fixed angle turns the tip and capsule ends stay round`() {
        val origin = MaskStamps.Centre(0.0, 0.0)
        val vertical = ellipse.copy(angleCentideg = 9000)
        assertTrue(MaskStamps.covers(1000, vertical, origin, 0.0, 500.0))
        assertFalse(MaskStamps.covers(1000, vertical, origin, 400.0, 0.0))
        assertFalse(MaskStamps.covers(1000, ellipse, origin, 250.0, 250.0))
        assertTrue(MaskStamps.covers(1000, ellipse.copy(tip = JsMaskBrushTip.CAPSULE), origin, 250.0, 250.0))
        assertFalse(MaskStamps.covers(1000, ellipse.copy(tip = JsMaskBrushTip.CAPSULE), origin, 500.0, 1.0))
    }

    @Test
    fun `coverage counts the stamp union instead of a round stroke`() {
        val shape = JsMaskShape.Stamped(1000, ellipse, listOf(JsMaskPoint(500, 500)))
        assertEquals(11 to 25, MaskCoverage.count(listOf(shape, shape), 1000, 1000))
        assertEquals(21 to 25, MaskCoverage.count(listOf(JsMaskShape.Stroke(1000, shape.points)), 1000, 1000))
    }

    @Test
    fun `followed brushes preserve the fixed tip and use top start coordinates`() {
        val shapes = MaskRules.followed(
            listOf(listOf(Offset(3f, 6f))), 400, Offset.Zero, 3f, 300f, ellipse,
        )
        assertEquals(listOf(JsMaskShape.Stamped(400, ellipse, listOf(JsMaskPoint(9900, 200)))), shapes)
    }
}
