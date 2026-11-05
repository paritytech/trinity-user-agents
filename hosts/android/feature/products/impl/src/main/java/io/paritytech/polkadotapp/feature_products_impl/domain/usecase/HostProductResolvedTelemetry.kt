package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionRegistry
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ProductId
import javax.inject.Inject

class HostProductResolvedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
    private val sessions: HostSessionRegistry,
) {
    fun startedAtMillis(): Long = timeProvider.nowMillis()

    fun report(productId: ProductId, startedAtMillis: Long, resolved: Boolean) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = if (resolved) HostEventOutcome.OK else HostEventOutcome.ERROR,
                    monotonicMs = now,
                    durationMs = now - startedAtMillis,
                    sessionId = sessions.sessionIdForProductOrSubname(productId.value),
                    tags = mapOf("product.id" to productId.value),
                )
            }
        }.logFailure("Failed to capture native host product resolved event")
    }

    companion object {
        const val EVENT_NAME = "host.product.resolved"
    }
}
