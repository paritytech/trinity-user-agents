package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import org.junit.Assert.assertEquals
import org.junit.Test

class MaskRulesTest {
    @Test
    fun `a collapsed node does not project touch points through an invalid inverse`() {
        val pan = mockk<LayoutCoordinates> { every { isAttached } returns true }
        val node = mockk<LayoutCoordinates> {
            every { isAttached } returns true
            every { localPositionOf(pan, any()) } returns Offset(Float.NaN, Float.NaN)
        }
        val coordinates = MaskCoordinates().apply { this.node = node }
        coordinates.update(pan)
        assertEquals(null, coordinates.projection)
    }

    @Test
    fun `followed points use the inverse press transform instead of a global offset`() {
        val frame = MaskProjection(Offset(0f, -50f), Offset(1f, 0f), Offset(0f, 2f))
        assertEquals(Offset(50f, 30f), frame.point(Offset(50f, 40f)))
        assertEquals(Offset(50f, 50f), frame.point(Offset(50f, 50f)))
    }

    @Test
    fun `a wire place is hundredths of a dp from the left, or from the right edge right to left`() {
        val point = JsMaskPoint(x = 1050, y = -200)

        assertEquals(Offset(31.5f, -6f), MaskRules.px(point, density = 3f, mirroredWidth = null))
        assertEquals(Offset(68.5f, -6f), MaskRules.px(point, density = 3f, mirroredWidth = 100f))
    }

    @Test
    fun `a fade follows the CSS ease curve from 0 to 1`() {
        assertEquals(0f, MaskRules.progress(now = 5, start = 10, duration = 350), 0f)
        assertEquals(1f, MaskRules.progress(now = 360, start = 10, duration = 350), 0f)
        assertEquals("ease is about 80% of the way at half time", 0.8024f, MaskRules.progress(185, 10, 350), 0.01f)
        assertEquals(1f, MaskRules.progress(now = 10, start = 10, duration = 0), 0f)
    }

    @Test
    fun `followed strokes move into the node and come back as hundredths of a dp`() {
        val strokes = listOf(listOf(Offset(3f, 6f), Offset(9f, 6f)), emptyList())

        val shapes = MaskRules.followed(strokes, width = 5200, place = Offset(30f, 0f), density = 3f, mirroredWidth = null)

        assertEquals(listOf(JsMaskShape.Stroke(5200, listOf(JsMaskPoint(1100, 200), JsMaskPoint(1300, 200)))), shapes)
    }

    @Test
    fun `right to left, followed strokes count from the node's right edge`() {
        val strokes = listOf(listOf(Offset(3f, 6f)))

        val shapes = MaskRules.followed(strokes, width = 400, place = Offset.Zero, density = 3f, mirroredWidth = 300f)

        assertEquals(listOf(JsMaskShape.Stroke(400, listOf(JsMaskPoint(9900, 200)))), shapes)
    }
}
