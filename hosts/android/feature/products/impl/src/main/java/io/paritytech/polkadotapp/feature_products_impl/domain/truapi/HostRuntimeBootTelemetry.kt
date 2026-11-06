package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import javax.inject.Inject

/** The shared truAPI runtime's first build: the suffix lookup, the session secret and the core's database. */
class HostRuntimeBootTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    fun nowMillis(): Long = timeProvider.nowMillis()

    fun report(startedAtMillis: Long, booted: Boolean) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = if (booted) HostEventOutcome.OK else HostEventOutcome.ERROR,
                    monotonicMs = now,
                    durationMs = now - startedAtMillis,
                    sessionId = null,
                )
            }
        }.logFailure("Failed to capture native host runtime boot event")
    }

    companion object {
        const val EVENT_NAME = "host.runtime.boot"
    }
}
