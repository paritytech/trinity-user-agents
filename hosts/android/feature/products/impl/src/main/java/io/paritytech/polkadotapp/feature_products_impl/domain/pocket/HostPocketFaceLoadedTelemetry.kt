package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import uniffi.truapi.ProductId
import javax.inject.Inject

class HostPocketFaceLoadedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    fun requestedAtMillis(): Long = timeProvider.nowMillis()

    fun reportCachedFace(productId: ProductId, requestedAtMillis: Long) =
        report(productId, requestedAtMillis, HostEventOutcome.CACHE_HIT)

    fun <T> reportFirstLiveFace(productId: ProductId, requestedAtMillis: Long, liveFaces: Flow<T>): Flow<T> {
        var shown = false
        return liveFaces
            .onEach {
                if (!shown) {
                    shown = true
                    report(productId, requestedAtMillis, HostEventOutcome.OK)
                }
            }
            .onCompletion { cause ->
                if (!shown && cause != null && cause !is CancellationException) {
                    report(productId, requestedAtMillis, HostEventOutcome.ERROR)
                }
            }
    }

    private fun report(productId: ProductId, requestedAtMillis: Long, outcome: HostEventOutcome) {
        runCatching {
            capture.capture {
                val now = timeProvider.nowMillis()
                originProvider.current().stepEvent(
                    name = EVENT_NAME,
                    outcome = outcome,
                    monotonicMs = now,
                    durationMs = now - requestedAtMillis,
                    sessionId = null,
                    tags = mapOf("product.id" to productId.value),
                )
            }
        }.logFailure("Failed to capture native host Pocket face event")
    }

    companion object {
        const val EVENT_NAME = "host.pocket.face_loaded"
    }
}
