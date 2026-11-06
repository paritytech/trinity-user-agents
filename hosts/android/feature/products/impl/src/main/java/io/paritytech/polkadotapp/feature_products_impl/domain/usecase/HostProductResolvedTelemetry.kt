package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ProductId
import javax.inject.Inject

class HostProductResolvedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
    private val cacheHits: HostCacheHits,
) {
    fun nowMillis(): Long = timeProvider.nowMillis()

    /**
     * A read from the chain, timed from when the resolve lock was taken; [queueMillis] is the wait for it.
     * A read that landed claims its session's report, so the lookups that waited on it stay silent; a
     * failed one claims nothing, so the next read still counts.
     */
    fun report(productId: ProductId, load: HostLoadContext, startedAtMillis: Long, queueMillis: Long, resolved: Boolean) {
        runCatching {
            if (capture.enabled && resolved && load.sessionId != null) cacheHits.isFirst(EVENT_NAME, productId.value, load)
            capture.capture {
                event(
                    productId,
                    load,
                    if (resolved) HostEventOutcome.OK else HostEventOutcome.ERROR,
                    startedAtMillis,
                    queueMillis,
                )
            }
        }.logFailure("Failed to capture native host product resolved event")
    }

    /**
     * A product this process already resolved, timed from the ask, reported once per trigger and
     * session. [queueMillis] is set when the answer arrived while this caller waited on the lock.
     */
    fun reportMemoryHit(productId: ProductId, load: HostLoadContext, askedAtMillis: Long, queueMillis: Long?) {
        runCatching {
            if (!capture.enabled || !cacheHits.isFirst(EVENT_NAME, productId.value, load)) return
            capture.capture { event(productId, load, HostEventOutcome.CACHE_HIT, askedAtMillis, queueMillis) }
        }.logFailure("Failed to capture native host product resolved event")
    }

    private fun event(
        productId: ProductId,
        load: HostLoadContext,
        outcome: HostEventOutcome,
        startedAtMillis: Long,
        queueMillis: Long?,
    ): HostEvent {
        val now = timeProvider.nowMillis()
        return originProvider.current().stepEvent(
            name = EVENT_NAME,
            outcome = outcome,
            monotonicMs = now,
            durationMs = now - startedAtMillis,
            sessionId = load.sessionId,
            tags = mapOf("product.id" to productId.value) + load.tags(),
            extras = queueMillis?.let { mapOf("queue_ms" to it) }.orEmpty(),
        )
    }

    companion object {
        const val EVENT_NAME = "host.product.resolved"
    }
}
