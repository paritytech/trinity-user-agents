package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import com.google.gson.JsonObject
import com.google.gson.JsonParser

/**
 * The counters the metrics log reports from one `sudo_unstable_metrics` answer, all since the
 * light client started. Byte and connection totals are the whole process's; [protocolBytes] is this
 * chain's received and sent bytes per network protocol. A counter the smoldot build lacks reads 0.
 */
data class LightClientMetricsReading(
    val bytesReceived: Double,
    val bytesSent: Double,
    val connectionsDialed: Double,
    val handshakesFinished: Double,
    val protocolBytes: Map<String, ProtocolBytes>,
) {
    data class ProtocolBytes(val received: Double, val sent: Double)

    companion object {
        fun parse(answer: String): LightClientMetricsReading {
            val families = JsonParser.parseString(answer).asJsonObject.getAsJsonArray("metrics")
                .map { it.asJsonObject }
            fun entries(name: String): List<JsonObject> = families
                .filter { it.get("name")?.asString == name }
                .flatMap { family -> family.getAsJsonArray("entries").map { it.asJsonObject } }
            fun total(name: String): Double = entries(name).sumOf { it.get("value").asDouble }

            val protocolBytes = entries("networkProtocolBytesTotal")
                .groupBy { it.getAsJsonObject("labels").get("protocol").asString }
                .mapValues { (_, byDirection) ->
                    fun direction(name: String) = byDirection
                        .filter { it.getAsJsonObject("labels").get("direction").asString == name }
                        .sumOf { it.get("value").asDouble }
                    ProtocolBytes(received = direction("in"), sent = direction("out"))
                }
            return LightClientMetricsReading(
                bytesReceived = total("networkBytesReceivedTotal"),
                bytesSent = total("networkBytesSentTotal"),
                connectionsDialed = total("networkConnectionsDialedTotal"),
                handshakesFinished = total("networkConnectionsHandshakesFinishedTotal"),
                protocolBytes = protocolBytes,
            )
        }
    }
}
