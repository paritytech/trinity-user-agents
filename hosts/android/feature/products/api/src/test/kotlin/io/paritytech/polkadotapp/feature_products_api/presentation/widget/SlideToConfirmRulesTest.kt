package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

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
}
