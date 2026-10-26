package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigDecimal

class PocketMotionTest {
    @Test
    fun `progress is eased across the range and clamped outside it`() {
        assertEquals(0f, PocketCollapse.progress(scrolledPx = -20f, rangePx = 150f), DELTA)
        assertEquals(0f, PocketCollapse.progress(scrolledPx = 0f, rangePx = 150f), DELTA)
        assertEquals(0.5f, PocketCollapse.progress(scrolledPx = 75f, rangePx = 150f), DELTA)
        assertEquals(0.15625f, PocketCollapse.progress(scrolledPx = 37.5f, rangePx = 150f), DELTA)
        assertEquals(1f, PocketCollapse.progress(scrolledPx = 150f, rangePx = 150f), DELTA)
        assertEquals(1f, PocketCollapse.progress(scrolledPx = Float.MAX_VALUE, rangePx = 150f), DELTA)
    }

    @Test
    fun `an empty range counts as docked`() {
        assertEquals(1f, PocketCollapse.progress(scrolledPx = 0f, rangePx = 0f), DELTA)
    }

    @Test
    fun `the card shrinks to the header width`() {
        assertEquals(1f, PocketCollapse.scale(0f), DELTA)
        assertEquals(60f / 366f, PocketCollapse.scale(1f), DELTA)
        assertEquals((1f + 60f / 366f) / 2f, PocketCollapse.scale(0.5f), DELTA)
    }

    @Test
    fun `the title and balance fade early`() {
        assertEquals(1f, PocketCollapse.titleAlpha(0f), DELTA)
        assertEquals(0.5f, PocketCollapse.titleAlpha(0.2f), DELTA)
        assertEquals(0f, PocketCollapse.titleAlpha(0.4f), DELTA)
        assertEquals(0f, PocketCollapse.titleAlpha(1f), DELTA)

        assertEquals(1f, PocketCollapse.balanceAlpha(0f), DELTA)
        assertEquals(0.1f, PocketCollapse.balanceAlpha(0.5f), DELTA)
        assertEquals(0f, PocketCollapse.balanceAlpha(1f), DELTA)
    }

    @Test
    fun `the card moves from its place in the list to the centre of the header`() {
        val width = 366f
        val height = 239f

        val (restX, restY) = PocketCollapse.offset(0f, 16f, 120f, 200f, 28f, width, height)
        assertEquals(16f, restX, DELTA)
        assertEquals(120f, restY, DELTA)

        val (dockX, dockY) = PocketCollapse.offset(1f, 16f, 120f, 200f, 28f, width, height)
        val docked = 60f / 366f
        assertEquals(200f, dockX + width * docked / 2f, DELTA)
        assertEquals(28f, dockY + height * docked / 2f, DELTA)

        val (midX, midY) = PocketCollapse.offset(0.5f, 16f, 120f, 200f, 28f, width, height)
        val half = PocketCollapse.scale(0.5f)
        assertEquals((16f + 200f - width * half / 2f) / 2f, midX, DELTA)
        assertEquals((120f + 28f - height * half / 2f) / 2f, midY, DELTA)
    }

    @Test
    fun `the count-up runs from the old figure to the new one, to the cent`() {
        val from = BigDecimal("10.00")
        val to = BigDecimal("60.00")

        assertEquals(BigDecimal("10.00"), countUpValue(from, to, 0f))
        assertEquals(BigDecimal("35.00"), countUpValue(from, to, 0.5f))
        assertEquals(BigDecimal("60.00"), countUpValue(from, to, 1f))
        assertEquals(BigDecimal("60.00"), countUpValue(from, to, 2f))
        assertEquals(BigDecimal("13.33"), countUpValue(BigDecimal.ZERO, BigDecimal("40"), 1f / 3f))
    }

    private companion object {
        const val DELTA = 1e-4f
    }
}
