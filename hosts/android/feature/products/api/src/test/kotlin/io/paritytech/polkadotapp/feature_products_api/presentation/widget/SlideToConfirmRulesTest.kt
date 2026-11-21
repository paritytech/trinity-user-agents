package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsSlideToConfirmVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.roundToInt

class SlideToConfirmRulesTest {
    private val ready = JsWidget.SlideToConfirm(text = "Slide to register", onConfirm = "register")
    private val slop = 8f

    @Test
    fun `a ready control with an action accepts a slide`() {
        assertTrue(SlideToConfirmRules.acceptsSlide(ready, heldAtEnd = false))
    }

    @Test
    fun `disabled, loading, confirmed and already slid controls accept no slide`() {
        assertFalse("disabled", SlideToConfirmRules.acceptsSlide(ready.copy(enabled = false), heldAtEnd = false))
        assertFalse("loading", SlideToConfirmRules.acceptsSlide(ready.copy(loading = true), heldAtEnd = false))
        assertFalse("confirmed", SlideToConfirmRules.acceptsSlide(ready.copy(confirmed = true), heldAtEnd = false))
        assertFalse("already slid", SlideToConfirmRules.acceptsSlide(ready, heldAtEnd = true))
    }

    // Sliding a control nothing listens to would leave it stuck at the end with no answer coming.
    @Test
    fun `a control without an action is inert`() {
        assertFalse(SlideToConfirmRules.acceptsSlide(ready.copy(onConfirm = null), heldAtEnd = false))
    }

    @Test
    fun `a sideways drag past the slop is the control's in both directions`() {
        assertEquals(SlideDragDecision.CLAIM, SlideToConfirmRules.decide(dx = 9f, dy = 2f, slop = slop))
        // Leftwards is swipe to reply's direction; starting on the control, it must not reach the bubble.
        assertEquals(SlideDragDecision.CLAIM, SlideToConfirmRules.decide(dx = -9f, dy = 2f, slop = slop))
    }

    @Test
    fun `a drag that is mostly up or down goes to the surface`() {
        assertEquals(SlideDragDecision.RELEASE, SlideToConfirmRules.decide(dx = 3f, dy = 9f, slop = slop))
        assertEquals(SlideDragDecision.RELEASE, SlideToConfirmRules.decide(dx = 9f, dy = -12f, slop = slop))
    }

    @Test
    fun `a movement inside the slop decides nothing yet`() {
        assertEquals(SlideDragDecision.WAIT, SlideToConfirmRules.decide(dx = 5f, dy = 5f, slop = slop))
    }

    @Test
    fun `the slide follows the reading direction and stays within the track`() {
        assertEquals(0.5f, SlideToConfirmRules.slideBy(progress = 0f, dx = 50f, travelPx = 100f, rtl = false), 0f)
        assertEquals(0.5f, SlideToConfirmRules.slideBy(progress = 0f, dx = -50f, travelPx = 100f, rtl = true), 0f)
        assertEquals(0f, SlideToConfirmRules.slideBy(progress = 0f, dx = -50f, travelPx = 100f, rtl = false), 0f)
        assertEquals(1f, SlideToConfirmRules.slideBy(progress = 0.8f, dx = 500f, travelPx = 100f, rtl = false), 0f)
    }

    @Test
    fun `a track not yet measured does not move`() {
        assertEquals(0.3f, SlideToConfirmRules.slideBy(progress = 0.3f, dx = 50f, travelPx = 0f, rtl = false), 0f)
    }

    @Test
    fun `only a release past the threshold completes the slide`() {
        assertTrue(SlideToConfirmRules.completes(progress = 0.9f, lifted = true))
        assertFalse("short of the threshold", SlideToConfirmRules.completes(progress = 0.89f, lifted = true))
        // A gesture the system cancelled, such as a scroll taking over, never confirms.
        assertFalse("not lifted", SlideToConfirmRules.completes(progress = 1f, lifted = false))
    }

    @Test
    fun `loading or confirmed pins the control at the end`() {
        assertEquals(SlideSettle.PIN_AT_END, SlideToConfirmRules.settle(heldAtEnd = false, loading = true, confirmed = false))
        assertEquals(SlideSettle.PIN_AT_END, SlideToConfirmRules.settle(heldAtEnd = true, loading = false, confirmed = true))
    }

    @Test
    fun `a tree that clears loading and confirmed sends a slid control back to the start`() {
        assertEquals(SlideSettle.RETURN_TO_START, SlideToConfirmRules.settle(heldAtEnd = true, loading = false, confirmed = false))
    }

    @Test
    fun `a control at the start stays there`() {
        assertEquals(SlideSettle.STAY, SlideToConfirmRules.settle(heldAtEnd = false, loading = false, confirmed = false))
    }

    @Test
    fun `a control given no height keeps the track's own 72, with the thumb 8 from its edges`() {
        assertEquals(72, SlideToConfirmRules.thumbTrackHeightDp(ready))
        assertEquals(8f, SlideToConfirmRules.thumbInsetDp(ready))
    }

    // The room footer docks its slider at 80, the height of every other docked card and of the web footer's pill.
    @Test
    fun `a control given the room footer's 80 stands 80, with the thumb 12 from its edges like the web pill`() {
        val docked = ready.copy(modifiers = listOf(JsModifier.FillMaxWidth(), JsModifier.Size(height = 80)))

        assertEquals(80, SlideToConfirmRules.thumbTrackHeightDp(docked))
        assertEquals(12f, SlideToConfirmRules.thumbInsetDp(docked))
    }

    @Test
    fun `a height too short for the thumb still holds the thumb`() {
        val squeezed = ready.copy(modifiers = listOf(JsModifier.Size(height = 40)))

        assertEquals(56, SlideToConfirmRules.thumbTrackHeightDp(squeezed))
        assertEquals(0f, SlideToConfirmRules.thumbInsetDp(squeezed))
    }

    // Compose sizes a node by the first height in its chain; the track must agree with what is drawn around it.
    @Test
    fun `the first height the product gives is the one the track takes`() {
        val twice = ready.copy(modifiers = listOf(JsModifier.Size(width = 300), JsModifier.Size(height = 80), JsModifier.Size(height = 64)))

        assertEquals(80, SlideToConfirmRules.thumbTrackHeightDp(twice))
    }

    // The room footer's track on a 1080 px phone at 2.625 px a dp: 954 px wide, the thumb 147 px, its inset 12 dp,
    // which the layout pads as 32 px. The thumb's lane is 954 - 2 * 32 = 890 px, so a full slide carries it 743 px.
    private val footerWidthPx = 954f
    private val thumbPx = 147f
    private val footerInsetPx = 32f
    private val footerTravelPx = SlideToConfirmRules.travelPx(footerWidthPx, endInsetPx = thumbPx, sideInsetPx = footerInsetPx)

    @Test
    fun `in the room footer a full slide carries the thumb along its lane inside the inset`() {
        assertEquals(743f, footerTravelPx, 0f)
    }

    @Test
    fun `the thumb keeps its inset from each side of the travel, the reveal keeps none`() {
        val docked = ready.copy(modifiers = listOf(JsModifier.Size(height = 80)))

        assertEquals(12f, SlideToConfirmRules.travelSideInsetDp(docked))
        assertEquals(8f, SlideToConfirmRules.travelSideInsetDp(ready))
        assertEquals(0f, SlideToConfirmRules.travelSideInsetDp(docked.copy(variant = JsSlideToConfirmVariant.REVEAL)))
    }

    // The face draws the thumb at progress times the travel, the same travel the drag divides the finger's move by.
    @Test
    fun `a finger move of N px moves the thumb N px before it reaches the end`() {
        for (move in listOf(1f, 21f, 100f, 371f, 742f)) {
            val progress = SlideToConfirmRules.slideBy(progress = 0f, dx = move, travelPx = footerTravelPx, rtl = false)
            val drawn = progress * footerTravelPx

            assertEquals("moved $move", move, drawn, 0.001f)
            assertEquals("moved $move, drawn on whole pixels", move.roundToInt(), drawn.roundToInt())
        }
    }

    @Test
    fun `moves add up one for one across a drag, in either reading direction`() {
        var ltr = 0f
        var rtl = 0f
        repeat(10) {
            ltr = SlideToConfirmRules.slideBy(progress = ltr, dx = 30f, travelPx = footerTravelPx, rtl = false)
            rtl = SlideToConfirmRules.slideBy(progress = rtl, dx = -30f, travelPx = footerTravelPx, rtl = true)
        }

        assertEquals(300f, ltr * footerTravelPx, 0.001f)
        assertEquals(300f, rtl * footerTravelPx, 0.001f)
    }

    @Test
    fun `a drag of the full travel reaches the end and released there completes`() {
        val progress = SlideToConfirmRules.slideBy(progress = 0f, dx = footerTravelPx, travelPx = footerTravelPx, rtl = false)

        assertEquals(1f, progress, 0f)
        assertEquals(743, (progress * footerTravelPx).roundToInt())
        assertTrue(SlideToConfirmRules.completes(progress, lifted = true))
    }

    @Test
    fun `the slide completes once released 90 percent of the drawn travel along, and not before`() {
        // 90% of 743 px is 668.7 px.
        val past = SlideToConfirmRules.slideBy(0f, dx = 669f, travelPx = footerTravelPx, rtl = false)
        val short = SlideToConfirmRules.slideBy(0f, dx = 668f, travelPx = footerTravelPx, rtl = false)

        assertTrue(SlideToConfirmRules.completes(past, lifted = true))
        assertFalse(SlideToConfirmRules.completes(short, lifted = true))
    }

    @Test
    fun `the reveal keeps its own travel, the width less the end it leaves unpeeled`() {
        assertEquals(807f, SlideToConfirmRules.travelPx(footerWidthPx, endInsetPx = thumbPx, sideInsetPx = 0f), 0f)
    }

    @Test
    fun `a track too narrow for the thumb does not travel`() {
        assertEquals(0f, SlideToConfirmRules.travelPx(200f, endInsetPx = thumbPx, sideInsetPx = footerInsetPx), 0f)
    }
}
