package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionRegistry
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import javax.inject.Inject

class HostBulletinFetchTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
    private val sessions: HostSessionRegistry,
) {
    fun nowMillis(): Long = timeProvider.nowMillis()

    fun report(
        domain: String,
        outcome: HostEventOutcome,
        contentHashMillis: Long,
        startedAtMillis: Long,
        bytes: Long?,
    ) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = outcome,
                    monotonicMs = now,
                    durationMs = now - startedAtMillis,
                    sessionId = sessions.sessionIdForProductOrSubname(domain),
                    extras = mapOf("dotns.domain" to domain, "dotns.content_hash_ms" to contentHashMillis) +
                        bytes?.let { mapOf("bulletin.bytes" to it) }.orEmpty(),
                )
            }
        }.logFailure("Failed to capture native host Bulletin fetch event")
    }

    companion object {
        const val EVENT_NAME = "host.bulletin.fetch"
    }
}
