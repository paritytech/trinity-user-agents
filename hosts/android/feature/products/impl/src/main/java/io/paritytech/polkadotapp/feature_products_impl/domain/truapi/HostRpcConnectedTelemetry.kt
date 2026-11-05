package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject

class HostRpcConnectedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    private val dialling = ConcurrentHashMap<UInt, Dial>()

    fun dialled(connectionId: UInt, chain: () -> String) {
        dialling[connectionId] = Dial(chain, timeProvider.nowMillis())
    }

    fun responded(connectionId: UInt) {
        dialling.remove(connectionId)?.let { report(it, HostEventOutcome.OK) }
    }

    fun closedBeforeResponse(connectionId: UInt) {
        dialling.remove(connectionId)?.let { report(it, HostEventOutcome.ERROR) }
    }

    fun abandoned(connectionId: UInt) {
        dialling.remove(connectionId)
    }

    private fun report(dial: Dial, outcome: HostEventOutcome) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = outcome,
                    monotonicMs = now,
                    durationMs = now - dial.startedAtMillis,
                    sessionId = null,
                    tags = mapOf("chain" to dial.chain()),
                )
            }
        }.logFailure("Failed to capture native host RPC connected event")
    }

    private class Dial(val chain: () -> String, val startedAtMillis: Long)

    companion object {
        const val EVENT_NAME = "host.rpc.connected"
    }
}
