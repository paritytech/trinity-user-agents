package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import javax.inject.Inject

class HostBulletinFetchTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
    private val cacheHits: HostCacheHits,
) {
    fun nowMillis(): Long = timeProvider.nowMillis()

    fun report(
        domain: String,
        load: HostLoadContext,
        outcome: HostEventOutcome,
        contentHashMillis: Long,
        startedAtMillis: Long,
        bytes: Long?,
    ) {
        runCatching {
            capture.capture {
                event(
                    domain,
                    load,
                    outcome,
                    startedAtMillis,
                    extras = mapOf("dotns.content_hash_ms" to contentHashMillis) +
                        bytes?.let { mapOf("bulletin.bytes" to it) }.orEmpty(),
                )
            }
        }.logFailure("Failed to capture native host Bulletin fetch event")
    }

    /** Content this process already resolved, reported once per trigger and session. */
    fun reportMemoryHit(domain: String, load: HostLoadContext, startedAtMillis: Long) {
        runCatching {
            if (!capture.enabled || !cacheHits.isFirst(EVENT_NAME, domain, load)) return
            capture.capture { event(domain, load, HostEventOutcome.CACHE_HIT, startedAtMillis) }
        }.logFailure("Failed to capture native host Bulletin fetch event")
    }

    private fun event(
        domain: String,
        load: HostLoadContext,
        outcome: HostEventOutcome,
        startedAtMillis: Long,
        extras: Map<String, Any> = emptyMap(),
    ): HostEvent {
        val now = timeProvider.nowMillis()
        return originProvider.current().stepEvent(
            name = EVENT_NAME,
            outcome = outcome,
            monotonicMs = now,
            durationMs = now - startedAtMillis,
            sessionId = load.sessionId,
            tags = load.tags(),
            extras = mapOf("dotns.domain" to domain) + extras,
        )
    }

    companion object {
        const val EVENT_NAME = "host.bulletin.fetch"
    }
}
