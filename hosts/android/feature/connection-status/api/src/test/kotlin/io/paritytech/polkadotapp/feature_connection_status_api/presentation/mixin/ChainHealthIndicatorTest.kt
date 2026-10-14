package io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin

import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

class ChainHealthIndicatorTest {
    private val blockTime = 2.seconds

    @Test
    fun `an unmeasured share is healthy with nothing to print`() {
        assertEquals(ChainHealthIndicator.Healthy(liveness = null), ChainHealthIndicator.of(null, blockTime))
    }

    @Test
    fun `five sixths of the expected blocks closes the disc`() {
        assertEquals(ChainHealthIndicator.Healthy(ChainLiveness(share = 1f, blockInterval = 2.seconds)), ChainHealthIndicator.of(1f, blockTime))
        assertEquals(ChainHealthIndicator.Healthy(ChainLiveness(share = 5f / 6f, blockInterval = 2.4.seconds)), ChainHealthIndicator.of(5f / 6f, blockTime))
    }

    @Test
    fun `below five sixths the arc carries the share unchanged`() {
        assertEquals(ChainHealthIndicator.Production(ChainLiveness(share = 0.8f, blockInterval = 2.5.seconds)), ChainHealthIndicator.of(0.8f, blockTime))
        assertEquals(ChainHealthIndicator.Production(ChainLiveness(share = 0.1f, blockInterval = 20.seconds)), ChainHealthIndicator.of(0.1f, blockTime))
    }

    @Test
    fun `the block interval is the block time over the share`() {
        assertEquals(20.seconds, ChainHealthIndicator.of(0.1f, blockTime).livenessOrNull()?.blockInterval)
        assertEquals(60.seconds, ChainHealthIndicator.of(0.1f, 6.seconds).livenessOrNull()?.blockInterval)
    }

    @Test
    fun `no blocks at all is the cross, not an arc`() {
        assertEquals(ChainHealthIndicator.Outage, ChainHealthIndicator.of(0f, blockTime))
    }

    @Test
    fun `the arc colour changes on the quarters`() {
        listOf(0f, 0.1f, 0.24f).forEach { assertEquals("$it", ProductionBand.Error, ProductionBand.of(it)) }
        listOf(0.25f, 0.4f, 0.49f).forEach { assertEquals("$it", ProductionBand.Warning, ProductionBand.of(it)) }
        listOf(0.5f, 0.74f, 0.8f).forEach { assertEquals("$it", ProductionBand.Plain, ProductionBand.of(it)) }
    }

    @Test
    fun `a share on a quarter belongs to the band above it`() {
        assertEquals(ProductionBand.Warning, production(0.25f).band)
        assertEquals(ProductionBand.Plain, production(0.5f).band)
    }

    private fun production(share: Float) = ChainHealthIndicator.of(share, blockTime) as ChainHealthIndicator.Production

    private fun ChainHealthIndicator.livenessOrNull(): ChainLiveness? = when (this) {
        is ChainHealthIndicator.Healthy -> liveness
        is ChainHealthIndicator.Production -> liveness
        else -> null
    }
}
