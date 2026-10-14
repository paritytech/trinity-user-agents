package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import org.junit.Assert.assertEquals
import org.junit.Test

private const val FULL_RING = 767

class RecyclerFungibilityCalculatorTest {
    @Test
    fun `a full ring with nothing unloaded is completely fungible`() {
        assertPercent(100, frozenMaxOf(capacity = FULL_RING, included = FULL_RING, unloaded = 0))
        assertPercent(100, currentOf(capacity = FULL_RING, included = FULL_RING, unloaded = 0))
    }

    @Test
    fun `a fully drained ring is not fungible at all`() {
        assertPercent(0, frozenMaxOf(capacity = FULL_RING, included = FULL_RING, unloaded = FULL_RING))
        assertPercent(0, currentOf(capacity = FULL_RING, included = FULL_RING, unloaded = FULL_RING))
    }

    /** The `I == 0` short circuit: no keys are baked in yet, so the voucher hides in nothing. */
    @Test
    fun `an empty ring is not fungible on either measure`() {
        assertEquals(RecyclerFungibility.NONE, frozenMaxOf(capacity = FULL_RING, included = 0, unloaded = 0))
        assertEquals(RecyclerFungibility.NONE, currentOf(capacity = FULL_RING, included = 0, unloaded = 0))
    }

    @Test
    fun `a half filled ring is half fungible today but fully fungible at capacity`() {
        assertPercent(50, currentOf(capacity = FULL_RING, included = 384, unloaded = 0))
        assertPercent(100, frozenMaxOf(capacity = FULL_RING, included = 384, unloaded = 0))
    }

    /**
     * `(64 - 1) / 2 = 31.5` exactly, so this pins the tie-break direction rather than the arithmetic.
     */
    @Test
    fun `a fungibility landing exactly on a half rounds up`() {
        assertPercent(32, currentOf(capacity = 25, included = 8, unloaded = 1))
    }

    /**
     * The runtime decrements the unloaded count when an alias is marked unloaded, so a stale read can carry
     * more unloaded keys than the ring admits to including. Clamping down to `I` keeps the result in range
     * instead of producing a negative numerator.
     */
    @Test
    fun `an unloaded count above the included count is clamped down to it`() {
        val clamped = currentOf(capacity = FULL_RING, included = 10, unloaded = 50)
        val atIncluded = currentOf(capacity = FULL_RING, included = 10, unloaded = 10)

        assertEquals(atIncluded, clamped)
        assertPercent(0, clamped)
    }

    @Test
    fun `an included count above capacity still yields a percentage in range`() {
        assertPercent(100, currentOf(capacity = 10, included = 100, unloaded = 0))
    }

    @Test
    fun `a capacity that could not be read is not fungible rather than a division by zero`() {
        assertEquals(RecyclerFungibility.NONE, frozenMaxOf(capacity = 0, included = 5, unloaded = 0))
        assertEquals(RecyclerFungibility.NONE, currentOf(capacity = 0, included = 5, unloaded = 0))
    }

    /** Unloading keys costs the ring more anonymity the emptier it already is — the squares, not a line. */
    @Test
    fun `unloading keys reduces fungibility faster than linearly`() {
        val quarterDrained = currentOf(capacity = 800, included = 800, unloaded = 200).percent
        val halfDrained = currentOf(capacity = 800, included = 800, unloaded = 400).percent

        assertEquals(94, quarterDrained)
        assertEquals(75, halfDrained)
    }

    private fun frozenMaxOf(capacity: Int, included: Int, unloaded: Int) =
        RecyclerFungibilityCalculator.maxFungibility(capacity, included, unloaded)

    private fun currentOf(capacity: Int, included: Int, unloaded: Int) =
        RecyclerFungibilityCalculator.fungibility(capacity, included, unloaded)

    private fun assertPercent(expected: Int, actual: RecyclerFungibility) =
        assertEquals(expected, actual.percent)
}
