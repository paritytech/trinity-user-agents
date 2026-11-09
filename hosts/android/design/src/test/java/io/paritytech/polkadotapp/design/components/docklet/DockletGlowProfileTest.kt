package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.exp

class DockletGlowProfileTest {
    @Test
    fun `the unblurred alpha is strongest at the edge and gone 24 dp in`() {
        assertEquals(0.56f, DockletGlowProfile.alpha(0f), EPS)
        assertEquals(0.5f * exp(-1f) + 0.06f * exp(-4.2f / 14f) * (1f - 4.2f / 24f), DockletGlowProfile.alpha(4.2f), EPS)
        assertEquals(0f, DockletGlowProfile.alpha(24f), EPS)
    }

    @Test
    fun `the blur leaves the screen edge dimmer than the band a few dp inside`() {
        val atEdge = DockletGlowProfile.glowAlpha(inFromEdge = 0f, along = Middle, length = Length)
        val inside = DockletGlowProfile.glowAlpha(inFromEdge = 2f, along = Middle, length = Length)

        assertTrue(atEdge < inside)
        assertTrue(atEdge < DockletGlowProfile.alpha(0f))
    }

    @Test
    fun `the glow is gone at the band's inner side`() {
        assertEquals(0f, DockletGlowProfile.glowAlpha(inFromEdge = 24f, along = Middle, length = Length), EPS)
    }

    @Test
    fun `away from its ends the glow keeps its full width`() {
        assertEquals(5f, DockletGlowProfile.taperedDistance(inFromEdge = 5f, along = 22f, length = Length), EPS)
        assertEquals(5f, DockletGlowProfile.taperedDistance(inFromEdge = 5f, along = Length - 22f, length = Length), EPS)
    }

    @Test
    fun `the ends taper along a quarter ellipse to nothing at the tip`() {
        assertEquals(0f, DockletGlowProfile.glowAlpha(inFromEdge = 0f, along = 0f, length = Length), EPS)
        assertEquals(12f, DockletGlowProfile.taperedDistance(inFromEdge = 0f, along = 11f, length = Length), EPS)
        assertEquals(
            DockletGlowProfile.glowAlpha(inFromEdge = 3f, along = 5f, length = Length),
            DockletGlowProfile.glowAlpha(inFromEdge = 3f, along = Length - 5f, length = Length),
            EPS,
        )
    }

    @Test
    fun `a glow shorter than both end radii rounds over half its length`() {
        assertEquals(0f, DockletGlowProfile.taperedDistance(inFromEdge = 0f, along = 10f, length = 20f), EPS)
        assertEquals(24f, DockletGlowProfile.taperedDistance(inFromEdge = 0f, along = 0f, length = 20f), EPS)
    }

    @Test
    fun `the shimmer is full at the edge and gone at the band's inner side`() {
        assertEquals(1f, DockletGlowProfile.shimmerStrength(inFromEdge = 0f, along = Middle, length = Length), EPS)
        assertEquals(0.5f, DockletGlowProfile.shimmerStrength(inFromEdge = 12f, along = Middle, length = Length), EPS)
        assertEquals(0f, DockletGlowProfile.shimmerStrength(inFromEdge = 30f, along = Middle, length = Length), EPS)
    }

    @Test
    fun `the glow narrows to half its width as the dock comes out`() {
        assertEquals(1f, DockletGlowProfile.widthScale(1f), EPS)
        assertEquals(0.75f, DockletGlowProfile.widthScale(0.5f), EPS)
        assertEquals(0.5f, DockletGlowProfile.widthScale(0f), EPS)
        assertEquals(0.5f, DockletGlowProfile.widthScale(-0.2f), EPS)
    }

    private companion object {
        const val EPS = 1e-3f
        const val Length = 200f
        const val Middle = 100f
    }
}
