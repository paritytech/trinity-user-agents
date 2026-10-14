package io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin.mapper

import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainHealth
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicator
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

class StatementStoreIndicatorMapperTest {
    @Test
    fun `a connected socket the peer has not answered is still connecting`() {
        assertEquals(
            ChainHealthIndicator.Connecting,
            health(ChainConnectionPresentation.Connected).statementStoreIndicator(answered = false),
        )
    }

    @Test
    fun `a connected socket the peer has answered is connected`() {
        assertEquals(
            ChainHealthIndicator.Healthy(liveness = null),
            health(ChainConnectionPresentation.Connected).statementStoreIndicator(answered = true),
        )
    }

    @Test
    fun `an answer never promotes a chain that is not connected`() {
        val states = mapOf(
            ChainConnectionPresentation.Connecting to ChainHealthIndicator.Connecting,
            ChainConnectionPresentation.Disconnected to ChainHealthIndicator.Disconnected,
            ChainConnectionPresentation.Offline to ChainHealthIndicator.Offline,
        )

        states.forEach { (connection, expected) ->
            assertEquals("$connection", expected, health(connection).statementStoreIndicator(answered = true))
            assertEquals("$connection", expected, health(connection).statementStoreIndicator(answered = false))
        }
    }

    @Test
    fun `an unmonitored chain leaves nothing to report`() {
        assertEquals(ChainHealthIndicator.Connecting, null.statementStoreIndicator(answered = true))
    }

    private fun health(connection: ChainConnectionPresentation) = ChainHealth(
        chainId = "people",
        chainName = "People",
        connection = connection,
        expectedBlockTime = 2.seconds,
        readings = emptyList(),
    )
}
