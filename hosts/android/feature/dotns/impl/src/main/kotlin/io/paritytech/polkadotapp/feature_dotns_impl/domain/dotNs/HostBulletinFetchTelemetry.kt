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

    /**
     * A read this lookup did itself. [queueMillis] is how long it waited for the cache, which another
     * caller's fetch holds. A read that landed claims its session's report, so the lookups that joined
     * it stay silent; a failed one claims nothing, so the next read still counts.
     */
    fun report(
        domain: String,
        load: HostLoadContext,
        outcome: HostEventOutcome,
        contentHashMillis: Long,
        startedAtMillis: Long,
        bytes: Long?,
        queueMillis: Long,
    ) {
        runCatching {
            if (capture.enabled && outcome != HostEventOutcome.ERROR && load.sessionId != null) {
                cacheHits.isFirst(EVENT_NAME, domain, load)
            }
            capture.capture {
                event(
                    domain,
                    load,
                    outcome,
                    startedAtMillis,
                    extras = mapOf("dotns.content_hash_ms" to contentHashMillis) +
                        bytes?.let { mapOf("bulletin.bytes" to it) }.orEmpty() +
                        (QUEUE_MS to queueMillis),
                )
            }
        }.logFailure("Failed to capture native host Bulletin fetch event")
    }

    /**
     * Content this process already resolved, or another caller was fetching, timed from the ask, all of
     * it spent waiting. Reported once per trigger and session.
     */
    fun reportMemoryHit(domain: String, load: HostLoadContext, askedAtMillis: Long) {
        runCatching {
            if (!capture.enabled || !cacheHits.isFirst(EVENT_NAME, domain, load)) return
            capture.capture {
                event(domain, load, HostEventOutcome.CACHE_HIT, askedAtMillis) { now ->
                    mapOf(QUEUE_MS to now - askedAtMillis)
                }
            }
        }.logFailure("Failed to capture native host Bulletin fetch event")
    }

    private fun event(
        domain: String,
        load: HostLoadContext,
        outcome: HostEventOutcome,
        startedAtMillis: Long,
        extras: Map<String, Any> = emptyMap(),
        timedExtras: (now: Long) -> Map<String, Any> = { emptyMap() },
    ): HostEvent {
        val now = timeProvider.nowMillis()
        return originProvider.current().stepEvent(
            name = EVENT_NAME,
            outcome = outcome,
            monotonicMs = now,
            durationMs = now - startedAtMillis,
            sessionId = load.sessionId,
            tags = load.tags(),
            extras = mapOf("dotns.domain" to domain) + extras + timedExtras(now),
        )
    }

    companion object {
        const val EVENT_NAME = "host.bulletin.fetch"
        const val QUEUE_MS = "queue_ms"
    }
}
