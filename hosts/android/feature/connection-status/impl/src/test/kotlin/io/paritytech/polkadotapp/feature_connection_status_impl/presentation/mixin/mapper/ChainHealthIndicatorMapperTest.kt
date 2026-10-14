package io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin.mapper

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainLiveness
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

class ChainHealthIndicatorMapperTest {
    @Test
    fun `no internet wins over everything`() {
        val health = health(ChainConnectionPresentation.Offline, blocks(15, 15), silentNode())

        assertEquals(ChainHealthIndicator.Offline, health.toIndicator())
    }

    @Test
    fun `a broken chain wins over everything`() {
        val health = health(ChainConnectionPresentation.Disconnected, blocks(15, 15), answeringNode())

        assertEquals(ChainHealthIndicator.Disconnected, health.toIndicator())
    }

    @Test
    fun `connecting wins over an outage`() {
        val health = health(ChainConnectionPresentation.Connecting, blocks(0, 15))

        assertEquals(ChainHealthIndicator.Connecting, health.toIndicator())
    }

    @Test
    fun `a node that stopped answering is broken even while the socket is connected`() {
        assertEquals(ChainHealthIndicator.Disconnected, connected(blocks(15, 15), silentNode()).toIndicator())
    }

    @Test
    fun `no readings is healthy with nothing measured`() {
        assertEquals(ChainHealthIndicator.Healthy(liveness = null), connected().toIndicator())
    }

    @Test
    fun `an unmeasured window is healthy with nothing measured`() {
        assertEquals(ChainHealthIndicator.Healthy(liveness = null), connected(blocks(null, 15), answeringNode()).toIndicator())
    }

    @Test
    fun `five sixths of the expected blocks is the line`() {
        assertEquals(ChainHealthIndicator.Healthy(ChainLiveness(share = 1f, blockInterval = 2.seconds)), connected(blocks(15, 15)).toIndicator())
        assertEquals(ChainHealthIndicator.Healthy(ChainLiveness(share = 13f / 15f, blockInterval = 2308.milliseconds)), connected(blocks(13, 15)).toIndicator())
        assertEquals(ChainHealthIndicator.Production(ChainLiveness(share = 0.8f, blockInterval = 2.5.seconds)), connected(blocks(12, 15)).toIndicator())
        assertEquals(ChainHealthIndicator.Production(ChainLiveness(share = 0.1f, blockInterval = 20.seconds)), connected(blocks(1, 10)).toIndicator())
    }

    @Test
    fun `nothing produced draws the cross`() {
        assertEquals(ChainHealthIndicator.Outage, connected(blocks(0, 15)).toIndicator())
    }

    private fun connected(vararg readings: ChainMetricReading): ChainHealth =
        health(ChainConnectionPresentation.Connected, *readings)

    private fun health(connection: ChainConnectionPresentation, vararg readings: ChainMetricReading): ChainHealth =
        ChainHealth(
            chainId = "people",
            chainName = "People",
            connection = connection,
            expectedBlockTime = 2.seconds,
            readings = readings.toList(),
        )

    private fun blocks(produced: Int?, expected: Int): ChainMetricReading = ChainMetricReading.BlockProduction(
        producedBlocks = produced,
        expectedBlocks = expected,
        anchorPending = false,
    )

    private fun silentNode(): ChainMetricReading = ChainMetricReading.UnansweredRequest(age = 7.seconds, limit = 6.seconds)

    private fun answeringNode(): ChainMetricReading = ChainMetricReading.UnansweredRequest(age = 1.seconds, limit = 6.seconds)
}
