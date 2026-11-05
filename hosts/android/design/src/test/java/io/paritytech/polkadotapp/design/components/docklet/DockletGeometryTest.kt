package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.hypot

class DockletGeometryTest {
    @Test
    fun `dock is the screen width minus 20 dp when the screen is 392 7 dp wide`() {
        val dock = gestureDock(screenWidth = 392.7f)

        assertDp(372.7f, dock.width)
        assertDp(10f, dock.left)
    }

    @Test
    fun `dock is not capped when the screen is 455 dp wide`() {
        assertDp(435f, gestureDock(screenWidth = 455f).width)
    }

    @Test
    fun `dock is 440 dp when the screen is 500 dp wide`() {
        assertDp(440f, gestureDock(screenWidth = 500f).width)
    }

    @Test
    fun `dock is centred when it is capped on a 600 dp screen`() {
        assertDp(80f, gestureDock(screenWidth = 600f).left)
    }

    @Test
    fun `dock bottom is the inset minus 7 dp when gesture navigation has a 24 dp inset`() {
        assertDp(17f, PhoneHeight.dp - gestureDock(inset = 24f).bottom)
    }

    @Test
    fun `dock bottom floors at 12 dp when gesture navigation has a 16 dp inset`() {
        assertDp(12f, PhoneHeight.dp - gestureDock(inset = 16f).bottom)
    }

    @Test
    fun `dock sits 8 dp above the bar when 3-button navigation has a 47 3 dp bar`() {
        assertDp(55.3f, PhoneHeight.dp - phoneRestDock().bottom)
    }

    @Test
    fun `dock sits 8 dp above the bar when 3-button navigation has a 16 dp bar`() {
        val dock = DockletGeometry.dockFrame(PhoneWidth.dp, PhoneHeight.dp, 16.dp, threeButtonNavigation = true)

        assertDp(24f, PhoneHeight.dp - dock.bottom)
    }

    @Test
    fun `Pill centre sits 22 dp below the dock top when the dock is at rest`() {
        val dock = phoneRestDock()
        val pill = DockletGeometry.pillFrame(dock)

        assertDp((dock.left + dock.width / 2).value, pill.left + pill.width / 2)
        assertDp((dock.top + 22.dp).value, pill.top + pill.height / 2)
        assertDp(64f, pill.width)
        assertDp(64f, pill.height)
    }

    @Test
    fun `cutout equals the brief's six curves when the dock is 382 dp wide`() {
        val cutout = DockletGeometry.cutout(centerX = 191f)

        assertPoint(154f, 0f, cutout.start)
        assertCurves(BriefCutout382, cutout.segments)
    }

    @Test
    fun `cutout moves only by the centre shift when the dock is 372 7 dp wide`() {
        val cutout = DockletGeometry.cutout(centerX = 372.7f / 2)
        val shifted = BriefCutout382.map { segment ->
            segment.map { (x, y) -> (x - 4.65f) to y }
        }

        assertPoint(154f - 4.65f, 0f, cutout.start)
        assertCurves(shifted, cutout.segments)
    }

    @Test
    fun `bowl leaves 2 dp around the Pill when the Pill sits 10 dp proud`() {
        val centre = 191f
        val cutout = DockletGeometry.cutout(centerX = centre)
        val bowl = listOf(cutout.segments[1].end to cutout.segments[2], cutout.segments[2].end to cutout.segments[3])
        val pillRadius = DockletGeometry.PillDiameter.value / 2
        val dock = phoneRestDock()
        val pill = DockletGeometry.pillFrame(dock)
        val pillCentreY = (pill.top + pill.height / 2 - dock.top).value

        val clearance = bowl.flatMap { (start, segment) -> sampleCubic(start, segment, BowlSamplesPerHalf) }
            .minOf { point -> hypot(point.x - centre, point.y - pillCentreY) - pillRadius }

        assertTrue("clearance $clearance", clearance in 1.95f..2.05f)
    }

    @Test
    fun `slot centres are 44, 114, 268 and 338 dp when the dock is 382 dp wide`() {
        assertDps(listOf(44f, 114f, 268f, 338f), DockletGeometry.slotCentres(382.dp))
    }

    @Test
    fun `slot centres follow the shoulders when the dock is 372 7 dp wide`() {
        assertDps(listOf(44f, 109.35f, 263.35f, 328.7f), DockletGeometry.slotCentres(372.7f.dp))
    }

    @Test
    fun `slots leave a gap when the dock is 440 dp wide`() {
        val centres = DockletGeometry.slotCentres(440.dp)
        val slotHalf = DockletGeometry.SlotWidth / 2

        assertDps(listOf(44f, 143f, 297f, 396f), centres)
        assertTrue(centres[1] - slotHalf > centres[0] + slotHalf)
    }

    @Test
    fun `slots overlap and keep 80 dp when the screen is 300 dp wide`() {
        val dock = gestureDock(screenWidth = 300f)

        assertDp(280f, dock.width)
        assertDps(listOf(44f, 63f, 217f, 236f), DockletGeometry.slotCentres(dock.width))
        assertDp(80f, DockletGeometry.SlotWidth)
    }

    @Test
    fun `touch spans meet at the midpoint when the dock is 372 7 dp wide`() {
        assertSpans(
            listOf(4f to 76.675f, 76.675f to 149.35f, 223.35f to 296.025f, 296.025f to 368.7f),
            DockletGeometry.touchSpans(372.7f.dp),
        )
    }

    @Test
    fun `touch spans are 64 dp when the screen is 358 dp wide`() {
        val spans = DockletGeometry.touchSpans(gestureDock(screenWidth = 358f).width)

        spans.forEach { assertDp(64f, it.width) }
    }

    @Test
    fun `touch spans shrink below 64 dp when the screen is 357 dp wide`() {
        val spans = DockletGeometry.touchSpans(gestureDock(screenWidth = 357f).width)

        spans.forEach { assertDp(63.75f, it.width) }
    }

    @Test
    fun `touch spans fill the 99 dp region when the screen is 300 dp wide`() {
        val spans = DockletGeometry.touchSpans(gestureDock(screenWidth = 300f).width)

        assertSpans(listOf(4f to 53.5f, 53.5f to 103f, 177f to 226.5f, 226.5f to 276f), spans)
        assertTrue(spans.all { it.start >= 4.dp && it.end <= 276.dp })
    }

    @Test
    fun `touch spans stay at least 64 dp when the screen is 500 dp wide`() {
        val spans = DockletGeometry.touchSpans(gestureDock(screenWidth = 500f).width)

        assertTrue(spans.all { it.width >= 64.dp })
        assertTrue(spans[0].end <= spans[1].start && spans[2].end <= spans[3].start)
    }

    @Test
    fun `card body ends 22 dp below the risen dock top when step 1 lays out`() {
        val dock = phoneRestDock()
        val card = DockletGeometry.step1CardFrame(dock)
        val risenTop = DockletGeometry.step1DockTop(dock)

        assertDp((dock.top - 2.dp).value, risenTop)
        assertDp((risenTop + 22.dp).value, card.bottom)
        assertDp(204f, card.height)
        assertDp(dock.left.value, card.left)
        assertDp(dock.width.value, card.width)
    }

    @Test
    fun `card top is the status inset plus 12 dp when fewer than 8 rows fit`() {
        val dock = phoneRestDock()
        val card = DockletGeometry.step2CardFrame(dock, keyboardTop = 520.dp, statusInset = 37.8f.dp)

        assertDp(49.8f, card.top)
        assertDp(516f, DockletGeometry.step2DockBottom(dock, keyboardTop = 520.dp))
        assertDp(476f, card.bottom)
    }

    @Test
    fun `card height is 0 when the keyboard leaves no room below the status inset`() {
        val card = DockletGeometry.step2CardFrame(phoneRestDock(), keyboardTop = 80.dp, statusInset = 37.8f.dp)

        assertDp(0f, card.height)
    }

    @Test
    fun `keyboard dock sits 4 dp above the keyboard top when the keyboard covers the rest dock`() {
        val dock = DockletGeometry.keyboardDock(phoneRestDock(), keyboardTop = 552.5f.dp)

        assertDp(548.5f, dock.bottom)
    }

    @Test
    fun `keyboard dock stays at rest when the keyboard is below the dock`() {
        val rest = phoneRestDock()

        assertEquals(rest, DockletGeometry.keyboardDock(rest, keyboardTop = PhoneHeight.dp))
    }

    @Test
    fun `dock bottom is 4 dp above the keyboard top when step 2 lays out`() {
        assertDp(548.5f, DockletGeometry.step2DockBottom(phoneRestDock(), keyboardTop = 552.5f.dp))
    }

    @Test
    fun `dock is back at its rest bottom when step 2 has no keyboard`() {
        val dock = phoneRestDock()
        val dockBottom = DockletGeometry.step2DockBottom(dock, keyboardTop = PhoneHeight.dp)
        val card = DockletGeometry.step2CardFrame(dock, keyboardTop = PhoneHeight.dp, statusInset = 37.8f.dp)

        assertDp(839.2f, dockBottom)
        assertDp(777.2f, dockBottom - DockletGeometry.DockHeight)
        assertDp(799.2f, card.bottom)
    }

    @Test
    fun `dock stays at its rest bottom when the keyboard top is below it`() {
        assertDp(839.2f, DockletGeometry.step2DockBottom(phoneRestDock(), keyboardTop = 850.dp))
    }

    @Test
    fun `dock rides the keyboard when its top is exactly 4 dp above the rest bottom`() {
        assertDp(839.2f, DockletGeometry.step2DockBottom(phoneRestDock(), keyboardTop = 843.2f.dp))
    }

    @Test
    fun `card keeps its top when the body is exactly 616 dp tall`() {
        val card = DockletGeometry.step2CardFrame(phoneRestDock(), keyboardTop = 696.dp, statusInset = 24.dp)

        assertDp(36f, card.top)
        assertDp(616f, card.height)
    }

    @Test
    fun `card top comes down when more than 8 rows would fit`() {
        val dock = DockletGeometry.dockFrame(PhoneWidth.dp, TallHeight.dp, 47.3f.dp, threeButtonNavigation = true)
        val near = DockletGeometry.step2CardFrame(dock, keyboardTop = 697.dp, statusInset = 24.dp)
        val far = DockletGeometry.step2CardFrame(dock, keyboardTop = 1000.dp, statusInset = 24.dp)

        assertDp(37f, near.top)
        assertDp(616f, near.height)
        assertDp(340f, far.top)
    }

    private fun gestureDock(screenWidth: Float = PhoneWidth, inset: Float = 24f) =
        DockletGeometry.dockFrame(screenWidth.dp, PhoneHeight.dp, inset.dp, threeButtonNavigation = false)

    private fun phoneRestDock() =
        DockletGeometry.dockFrame(PhoneWidth.dp, PhoneHeight.dp, 47.3f.dp, threeButtonNavigation = true)

    private fun sampleCubic(start: DockletPoint, segment: CubicSegment, samples: Int): List<DockletPoint> =
        (0..samples).map { index -> cubicPoint(start, segment, index.toFloat() / samples) }

    private fun assertDp(expected: Float, actual: Dp) = assertEquals(expected, actual.value, EPS)

    private fun assertDps(expected: List<Float>, actual: List<Dp>) {
        assertEquals(expected.size, actual.size)
        expected.zip(actual).forEach { (e, a) -> assertDp(e, a) }
    }

    private fun assertSpans(expected: List<Pair<Float, Float>>, actual: List<DockletSpan>) {
        assertEquals(expected.size, actual.size)
        expected.zip(actual).forEach { (e, a) ->
            assertDp(e.first, a.start)
            assertDp(e.second, a.end)
        }
    }

    private companion object {
        const val EPS = GeometryEps
        const val PhoneWidth = 392.7f
        const val PhoneHeight = 894.5f
        const val TallHeight = 1100f
        const val BowlSamplesPerHalf = 100

        val BriefCutout382 = listOf(
            listOf(158f to 0f, 160f to 5f, 159f to 9f),
            listOf(158f to 13f, 157f to 17f, 157f to 22f),
            listOf(157f to 41f, 172f to 56f, 191f to 56f),
            listOf(210f to 56f, 225f to 41f, 225f to 22f),
            listOf(225f to 17f, 224f to 13f, 223f to 9f),
            listOf(222f to 5f, 224f to 0f, 228f to 0f),
        )
    }
}
