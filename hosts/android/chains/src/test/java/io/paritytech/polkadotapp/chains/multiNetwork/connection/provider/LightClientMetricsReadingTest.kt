package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import io.mockk.mockk
import io.parity.truapi.LightClient
import io.paritytech.polkadotapp.chains.multiNetwork.connection.provider.LightClientMetricsReading.ProtocolTraffic
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainMessageListener

class LightClientMetricsReadingTest {
    // The log compares phone runs with desktop ones by these numbers, so each must come from the
    // family and label smoldot reports it under, and directions must not be swapped.
    @Test
    fun `a metrics answer reads into byte, connection and per-protocol totals`() {
        val answer = """
            {"metrics": [
              {"name": "networkBytesReceivedTotal", "type": "counter", "entries": [{"labels": {}, "value": 3660000}]},
              {"name": "networkBytesSentTotal", "type": "counter", "entries": [{"labels": {}, "value": 170000}]},
              {"name": "networkConnectionsDialedTotal", "type": "counter", "entries": [{"labels": {}, "value": 198}]},
              {"name": "networkConnectionsHandshakesFinishedTotal", "type": "counter", "entries": [{"labels": {}, "value": 16}]},
              {"name": "networkProtocolBytesTotal", "type": "counter", "entries": [
                {"labels": {"protocol": "storage-proof", "direction": "in"}, "value": 340000},
                {"labels": {"protocol": "storage-proof", "direction": "out"}, "value": 40000},
                {"labels": {"protocol": "statements", "direction": "in"}, "value": 10000},
                {"labels": {"protocol": "statements", "direction": "out"}, "value": 0}
              ]},
              {"name": "networkProtocolMessagesTotal", "type": "counter", "entries": [
                {"labels": {"protocol": "storage-proof", "direction": "in"}, "value": 12}
              ]}
            ]}
        """.trimIndent()

        assertEquals(
            LightClientMetricsReading(
                bytesReceived = 3_660_000.0,
                bytesSent = 170_000.0,
                connectionsDialed = 198.0,
                handshakesFinished = 16.0,
                protocolTraffic = mapOf(
                    "storage-proof" to ProtocolTraffic(received = 340_000.0, sent = 40_000.0, messagesReceived = 12.0),
                    "statements" to ProtocolTraffic(received = 10_000.0, sent = 0.0, messagesReceived = 0.0),
                ),
            ),
            LightClientMetricsReading.parse(answer),
        )
    }

    // The app's own smoldot build must answer in the shape the log reads, and only for chains the
    // light client is running.
    @Test
    fun `the light client reports metrics for the chains it runs`() = runBlocking {
        val people = "4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48"
        val bulletin = "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22"
        val lightClient = LightClient(storage = mockk(relaxed = true), chains = setOf(people, bulletin))
        val ignored = object : ChainMessageListener {
            override fun onMessage(message: String) = Unit
            override fun onClosed(reason: ChainCloseReason) = Unit
        }

        lightClient.connect(people.hexToByteArray(), ignored).use {
            val metrics = lightClient.metrics()

            assertEquals(setOf(people), metrics.keys)
            LightClientMetricsReading.parse(metrics.getValue(people))
        }
        Unit
    }
}
