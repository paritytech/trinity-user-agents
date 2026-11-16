package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private const val TOUCH = 50_000L

class ProductOverlayTouchRuleTest {
    @Test
    fun `an overlay just after a touch on the footer is honoured`() {
        val rule = ProductOverlayTouchRule().apply { touchEnded(TOUCH, insideFooter = true) }

        assertTrue(rule.allowsOverlay(TOUCH))
        assertTrue(rule.allowsOverlay(TOUCH + 900))
        assertTrue(rule.allowsOverlay(TOUCH + ProductOverlayTouchRule.WINDOW_MILLIS))
    }

    @Test
    fun `an overlay with no touch is dropped`() {
        assertFalse(ProductOverlayTouchRule().allowsOverlay(TOUCH))
    }

    @Test
    fun `an overlay after the window is dropped`() {
        val rule = ProductOverlayTouchRule().apply { touchEnded(TOUCH, insideFooter = true) }

        assertFalse(rule.allowsOverlay(TOUCH + ProductOverlayTouchRule.WINDOW_MILLIS + 1))
        assertFalse(rule.allowsOverlay(TOUCH + 10_000))
    }

    @Test
    fun `a touch that ended outside the footer does not count`() {
        val rule = ProductOverlayTouchRule().apply { touchEnded(TOUCH, insideFooter = false) }

        assertFalse(rule.allowsOverlay(TOUCH + 100))
    }

    @Test
    fun `a later touch outside the footer keeps the earlier one`() {
        val rule = ProductOverlayTouchRule().apply {
            touchEnded(TOUCH, insideFooter = true)
            touchEnded(TOUCH + 500, insideFooter = false)
        }

        assertTrue(rule.allowsOverlay(TOUCH + 800))
    }
}
