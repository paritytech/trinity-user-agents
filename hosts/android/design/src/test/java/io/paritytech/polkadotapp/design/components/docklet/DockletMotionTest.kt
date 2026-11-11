package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.core.CubicBezierEasing
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

class DockletMotionTest {
    @Test
    fun `height progress is 0 875 when t is 0 5`() {
        assertEquals(0.875f, DockletMotion.heightProgress(0.5f), EPS)
    }

    @Test
    fun `width progress is 0 when t is 0 02`() {
        assertEquals(0f, DockletMotion.widthProgress(0.02f), EPS)
    }

    @Test
    fun `width progress is 0 when t is 0 04`() {
        assertEquals(0f, DockletMotion.widthProgress(0.04f), EPS)
    }

    @Test
    fun `width progress is 0 7222 when t is 0 5`() {
        assertEquals(0.72223f, DockletMotion.widthProgress(0.5f), EPS)
    }

    @Test
    fun `width progress is 1 when t is 1`() {
        assertEquals(1f, DockletMotion.widthProgress(1f), EPS)
    }

    @Test
    fun `contents are invisible when 223 ms have passed since a step change`() {
        val frame = DockletMotion.contentsIn(223.milliseconds)

        assertEquals(0f, frame.alpha, EPS)
        assertEquals(8f, frame.rise.value, EPS)
    }

    @Test
    fun `contents follow the (0 2, 0 8, 0 2, 1) curve when 294 5 ms have passed`() {
        val expected = CubicBezierEasing(0.2f, 0.8f, 0.2f, 1f).transform(0.5f)
        val frame = DockletMotion.contentsIn(294.5.milliseconds)

        assertEquals(expected, frame.alpha, EPS)
        assertEquals(8f * (1f - expected), frame.blurSigma.value, EPS)
    }

    @Test
    fun `contents blur is 8 dp when the fade-in starts`() {
        val frame = DockletMotion.contentsIn(224.milliseconds)

        assertEquals(8f, frame.blurSigma.value, EPS)
        assertEquals(0f, frame.alpha, EPS)
    }

    @Test
    fun `contents blur is 1 862 dp when 35 25 ms of the fade-in have passed`() {
        val frame = DockletMotion.contentsIn(259.25.milliseconds)

        assertEquals(1.862f, frame.blurSigma.value, EPS)
        assertEquals(0.7673f, frame.alpha, EPS)
    }

    @Test
    fun `contents blur is 0 when 141 ms of the fade-in have passed`() {
        val frame = DockletMotion.contentsIn(365.milliseconds)

        assertEquals(0f, frame.blurSigma.value, EPS)
        assertEquals(1f, frame.alpha, EPS)
    }

    @Test
    fun `contents are fully in when 365 ms have passed since a step change`() {
        val frame = DockletMotion.contentsIn(365.milliseconds)

        assertEquals(1f, frame.alpha, EPS)
        assertEquals(0f, frame.rise.value, EPS)
        assertEquals(0f, frame.blurSigma.value, EPS)
    }

    @Test
    fun `contents are gone when 120 ms have passed since a close started`() {
        assertEquals(0f, DockletMotion.contentsOutAlpha(120.milliseconds), EPS)
        assertTrue(DockletMotion.contentsOutAlpha(60.milliseconds) > 0f)
    }

    @Test
    fun `outline has not moved when 119 ms have passed since a close started`() {
        assertEquals(1f, DockletMotion.closeOutline(119.milliseconds), EPS)
    }

    @Test
    fun `outline is at q 0 125 when 284 5 ms have passed since a close started`() {
        assertEquals(0.125f, DockletMotion.closeOutline(284.5.milliseconds), EPS)
    }

    @Test
    fun `outline is home when 449 ms have passed since a close started`() {
        assertEquals(0f, DockletMotion.closeOutline(449.milliseconds), EPS)
    }

    @Test
    fun `swipe contents stay hidden when swipe progress is 0 87`() {
        assertEquals(0f, DockletMotion.swipeContentsAlpha(0.87f), EPS)
    }

    @Test
    fun `swipe contents are half in when swipe progress is 0 94`() {
        assertEquals(0.5f, DockletMotion.swipeContentsAlpha(0.94f), EPS)
    }

    @Test
    fun `contents-in easing holds 0 when the transition is still inside the delay`() {
        assertEquals(0f, DockletMotion.ContentsInEasing.transform(0.6f), EPS)
    }

    @Test
    fun `contents-out easing follows the ease-out when half the fade has passed`() {
        assertEquals(0.875f, DockletMotion.ContentsOutEasing.transform(0.5f), EPS)
    }

    @Test
    fun `contents-out easing reaches 1 when the fade completes`() {
        assertEquals(1f, DockletMotion.ContentsOutEasing.transform(1f), EPS)
    }

    private companion object {
        const val EPS = 1e-3f
    }
}
