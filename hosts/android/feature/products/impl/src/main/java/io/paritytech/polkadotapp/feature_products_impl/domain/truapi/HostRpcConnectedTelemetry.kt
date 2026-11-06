package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ChainConnectionObserver
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import javax.inject.Inject

/** The product execution a chain connection was dialled for. */
class HostRpcExecutable(
    val productId: ProductId,
    val kind: ExecutableKind,
    val sessionId: String?,
)

/**
 * Times each chain connection one chain provider dials, from the dial to its first frame. Connection
 * ids are only unique within one provider, so each provider takes its own instance: this is unscoped.
 */
class HostRpcConnectedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    private val dialling = ConcurrentHashMap<UInt, Dial>()

    fun observer(
        executable: () -> HostRpcExecutable?,
        chainLabel: (genesisHash: ByteArray) -> String,
    ): ChainConnectionObserver = object : ChainConnectionObserver {
        override fun dialling(connectionId: UInt, genesisHash: ByteArray, attempt: Int) {
            if (!capture.enabled) return
            if (attempt == 0) {
                dialling[connectionId] = Dial({ chainLabel(genesisHash) }, executable(), timeProvider.nowMillis())
            } else {
                dialling[connectionId]?.failovers?.incrementAndGet()
            }
        }

        override fun closingByHost(connectionId: UInt) {
            dialling.remove(connectionId)?.let { report(it, HostEventOutcome.ABANDONED) }
        }
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
                    sessionId = dial.executable?.sessionId,
                    tags = mapOf("rpc.chain" to dial.chain()) + dial.executable?.tags().orEmpty(),
                    extras = mapOf("failover_count" to dial.failovers.get()),
                )
            }
        }.logFailure("Failed to capture native host RPC connected event")
    }

    private fun HostRpcExecutable.tags() = mapOf(
        "product.id" to productId.value,
        "executable.kind" to if (kind == ExecutableKind.WORKER) "worker" else "page",
    )

    private class Dial(val chain: () -> String, val executable: HostRpcExecutable?, val startedAtMillis: Long) {
        val failovers = AtomicInteger(0)
    }

    companion object {
        const val EVENT_NAME = "host.rpc.connected"
    }
}
