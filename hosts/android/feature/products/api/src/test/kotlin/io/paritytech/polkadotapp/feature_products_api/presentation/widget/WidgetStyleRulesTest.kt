package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.TweenSpec
import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WidgetStyleRulesTest {
    @Test
    fun `a colour by value reads 0xRRGGBBAA`() {
        assertEquals(Color(0xFF33363A), WidgetStyleRules.color(0x33363AFFL))
        assertEquals(Color(0x5C000000), WidgetStyleRules.color(0x0000005CL))
    }

    @Test
    fun `the text fades as the existing slider's does, gone from five sevenths on`() {
        assertEquals(1f, SlideStyle.textOpacity(0f), 0f)
        assertEquals(0.3f, SlideStyle.textOpacity(0.5f), 0.0001f)
        assertEquals(0f, SlideStyle.textOpacity(5f / 7f), 0.0001f)
        assertEquals(0f, SlideStyle.textOpacity(1.2f), 0f)
        assertEquals(1f, SlideStyle.textOpacity(-0.3f), 0f)
    }

    @Test
    fun `the band sweeps from lead before the text to lead past it, then rests`() {
        val shimmer = SlideStyle.shimmer
        // A 160-wide line: (160 + 140) / 212 s of sweep, then 1 s at rest.
        val sweepMs = (160f + 140f) / 212f * 1000f
        assertEquals(-70f, WidgetStyleRules.shimmerCenter(shimmer, 160f, 0L)!!, 0.001f)
        assertEquals(80f, WidgetStyleRules.shimmerCenter(shimmer, 160f, (sweepMs / 2).toLong())!!, 0.2f)
        assertNull(WidgetStyleRules.shimmerCenter(shimmer, 160f, (sweepMs + 500).toLong()))
        assertEquals(-70f, WidgetStyleRules.shimmerCenter(shimmer, 160f, (sweepMs + 1000).toLong() + 1)!!, 0.3f)
    }

    @Test
    fun `a CSS blur of 6 is a Gaussian deviation of 3`() {
        // Skia: deviation = 0.57735 × radius + 0.5.
        val radius = WidgetStyleRules.blurMaskRadius(6f)
        assertEquals(3f, 0.57735f * radius + 0.5f, 0.0001f)
        assertEquals(0f, WidgetStyleRules.blurMaskRadius(0f), 0f)
    }

    @Test
    fun `the approach to the end takes 117 ms up to 42 dp, longer as the square root beyond`() {
        val feel = SlideStyle.feel
        assertEquals(117, WidgetStyleRules.approachMs(feel, 10f))
        assertEquals(234, WidgetStyleRules.approachMs(feel, 168f))
    }

    @Test
    fun `the pop runs the existing slider's table, peaks 3_33 past the end, and ends at rest`() {
        val feel = SlideStyle.feel
        assertEquals(1.059f, WidgetStyleRules.pop(feel, 0f, 1.059f)!!.first, 0f)
        val peak = WidgetStyleRules.pop(feel, 5f, 1.059f)!!
        assertEquals(1.237f, peak.first, 0.0001f)
        assertEquals(3.33f, peak.second, 0.0001f)
        val under = WidgetStyleRules.pop(feel, 20f, 1.059f)!!
        assertEquals(0.976f, under.first, 0.0001f)
        assertEquals(0f, under.second, 0f)
        assertNull(WidgetStyleRules.pop(feel, 27f, 1.059f))
    }

    @Test
    fun `a released every-move thumb settles with its pan's feel`() {
        val feel = SlideStyle.feel
        val home = settleSpec(WorkerThumbSettle.Kind.HOME, feel, 120f) as SpringSpec<*>
        assertEquals(0.8f, home.dampingRatio, 0f)
        assertEquals(324f, home.stiffness, 0.001f)
        val end = settleSpec(WorkerThumbSettle.Kind.END, feel, 10f) as TweenSpec<*>
        assertEquals(117, end.durationMillis)
        assertEquals(WidgetStyleRules.EaseOutQuad, end.easing)
        assertTrue(settleSpec(WorkerThumbSettle.Kind.HOME, null, 120f) is SpringSpec<*>)
    }
}
