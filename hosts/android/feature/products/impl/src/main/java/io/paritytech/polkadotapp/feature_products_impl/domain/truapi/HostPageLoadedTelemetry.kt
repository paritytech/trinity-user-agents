package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ProductId
import javax.inject.Inject

class HostPageLoadedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    fun openRequestedAtMillis(): Long = timeProvider.nowMillis()

    fun report(sessionId: String, productId: ProductId, openRequestedAtMillis: Long) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = HostEventOutcome.OK,
                    monotonicMs = now,
                    durationMs = now - openRequestedAtMillis,
                    sessionId = sessionId,
                    tags = mapOf("product.id" to productId.value),
                )
            }
        }.logFailure("Failed to capture native host page loaded event")
    }

    companion object {
        const val EVENT_NAME = "host.webview.page_loaded"
    }
}
