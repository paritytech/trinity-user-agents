package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ProductId
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject

/** One worker activation's boot, from the core's demand to the entry module being handed over. */
class HostWorkerBootTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    fun nowMillis(): Long = timeProvider.nowMillis()

    fun begin(productId: ProductId, sessionId: String, demandedAtMillis: Long) =
        WorkerBoot(productId, sessionId, demandedAtMillis)

    inner class WorkerBoot internal constructor(
        private val productId: ProductId,
        private val sessionId: String,
        private val demandedAtMillis: Long,
    ) {
        private val phases = ConcurrentHashMap<String, Long>()
        private val reported = AtomicBoolean(false)

        /** Records how long phase [key] took, from [startedAtMillis] to now. Only its first run counts. */
        fun phase(key: String, startedAtMillis: Long) {
            phases.putIfAbsent(key, timeProvider.nowMillis() - startedAtMillis)
        }

        /** Reports the boot once, as ended at [endedAtMillis], read where the boot returned. */
        fun finish(outcome: HostEventOutcome, endedAtMillis: Long) {
            if (!reported.compareAndSet(false, true)) return
            runCatching {
                capture.capture {
                    originProvider.current().stepEvent(
                        name = EVENT_NAME,
                        outcome = outcome,
                        monotonicMs = endedAtMillis,
                        durationMs = endedAtMillis - demandedAtMillis,
                        sessionId = sessionId,
                        tags = mapOf("product.id" to productId.value),
                        extras = phases.toMap(),
                    )
                }
            }.logFailure("Failed to capture native host worker boot event")
        }
    }

    companion object {
        const val EVENT_NAME = "host.worker.boot"
        const val RESOLVE_MS = "resolve_ms"
        const val WEBVIEW_CREATE_MS = "webview_create_ms"
        const val EXECUTION_OPEN_MS = "execution_open_ms"
        const val BLANK_READY_MS = "blank_ready_ms"
    }
}
