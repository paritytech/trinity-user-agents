package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import com.google.gson.JsonObject
import com.google.gson.JsonParser

/**
 * The counters the metrics log reports from one `sudo_unstable_metrics` answer, all since the
 * light client started. Byte and connection totals are the whole process's; [protocolTraffic] is
 * this chain's received and sent bytes and received messages per network protocol (for a request
 * protocol such as storage proofs, a received message is one response). A counter the smoldot build
 * lacks reads 0.
 */
data class LightClientMetricsReading(
    val bytesReceived: Double,
    val bytesSent: Double,
    val connectionsDialed: Double,
    val handshakesFinished: Double,
    val protocolTraffic: Map<String, ProtocolTraffic>,
) {
    data class ProtocolTraffic(val received: Double, val sent: Double, val messagesReceived: Double)

    companion object {
        fun parse(answer: String): LightClientMetricsReading {
            val families = JsonParser.parseString(answer).asJsonObject.getAsJsonArray("metrics")
                .map { it.asJsonObject }
            fun entries(name: String): List<JsonObject> = families
                .filter { it.get("name")?.asString == name }
                .flatMap { family -> family.getAsJsonArray("entries").map { it.asJsonObject } }
            fun total(name: String): Double = entries(name).sumOf { it.get("value").asDouble }
            fun JsonObject.label(name: String): String = getAsJsonObject("labels").get(name).asString
            fun perProtocol(family: String, direction: String): Map<String, Double> = entries(family)
                .filter { it.label("direction") == direction }
                .groupBy { it.label("protocol") }
                .mapValues { (_, protocolEntries) -> protocolEntries.sumOf { it.get("value").asDouble } }

            val received = perProtocol("networkProtocolBytesTotal", "in")
            val sent = perProtocol("networkProtocolBytesTotal", "out")
            val messagesReceived = perProtocol("networkProtocolMessagesTotal", "in")
            val protocolTraffic = (received.keys + sent.keys).associateWith { protocol ->
                ProtocolTraffic(
                    received = received[protocol] ?: 0.0,
                    sent = sent[protocol] ?: 0.0,
                    messagesReceived = messagesReceived[protocol] ?: 0.0,
                )
            }
            return LightClientMetricsReading(
                bytesReceived = total("networkBytesReceivedTotal"),
                bytesSent = total("networkBytesSentTotal"),
                connectionsDialed = total("networkConnectionsDialedTotal"),
                handshakesFinished = total("networkConnectionsHandshakesFinishedTotal"),
                protocolTraffic = protocolTraffic,
            )
        }
    }
}
