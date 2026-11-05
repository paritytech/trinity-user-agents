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

    fun report(fetch: BulletinFetch) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = fetch.outcome,
                    monotonicMs = now,
                    durationMs = now - fetch.startedAtMillis,
                    sessionId = sessions.sessionIdForProductOrSubname(fetch.domain),
                    tags = mapOf("dotns.domain" to fetch.domain),
                    extras = mapOf("dotns.content_hash_ms" to fetch.contentHashMillis) +
                        fetch.bytes?.let { mapOf("bulletin.bytes" to it) }.orEmpty(),
                )
            }
        }.logFailure("Failed to capture native host Bulletin fetch event")
    }

    companion object {
        const val EVENT_NAME = "host.bulletin.fetch"
    }
}

class BulletinFetch(
    val domain: String,
    val outcome: HostEventOutcome,
    val contentHashMillis: Long,
    val startedAtMillis: Long,
    val bytes: Long?,
)
