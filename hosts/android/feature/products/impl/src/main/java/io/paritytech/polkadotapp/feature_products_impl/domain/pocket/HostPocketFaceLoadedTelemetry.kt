package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOutcome
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.stepEvent
import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

class HostPocketFaceLoadedTelemetry @Inject internal constructor(
    private val originProvider: HostEventOriginProvider,
    private val timeProvider: HostMonotonicTimeProvider,
    private val capture: HostEventCapture,
) {
    /** One request for [productId]'s face, made now. */
    fun start(productId: ProductId): FaceLoad = FaceLoad(productId, timeProvider.nowMillis())

    /**
     * Reports a kept face as a cache hit, and then exactly one outcome for the live face: ok, error,
     * no_worker, timeout, or abandoned when the Pocket stops asking first.
     */
    inner class FaceLoad internal constructor(
        private val productId: ProductId,
        private val requestedAtMillis: Long,
    ) : PocketFaceLoad {
        @Volatile private var warm: Boolean? = null

        @Volatile private var sessionId: () -> String? = { null }
        private val finished = AtomicBoolean(false)

        fun cachedFace() = report(HostEventOutcome.CACHE_HIT)

        override fun worker(warm: Boolean, sessionId: () -> String?) {
            this.warm = warm
            this.sessionId = sessionId
        }

        override fun noWorker() = finish(HostEventOutcome.NO_WORKER)

        /**
         * Passes [liveFaces] through unchanged, reporting the first. The timeout only reports: the
         * stream keeps waiting for its worker as it always did.
         */
        fun <T> track(liveFaces: Flow<T>): Flow<T> = flow {
            coroutineScope {
                val timeout = if (capture.enabled) {
                    launch {
                        delay(FACE_TIMEOUT)
                        finish(HostEventOutcome.TIMEOUT)
                    }
                } else {
                    null
                }
                try {
                    liveFaces.collect { face ->
                        finish(HostEventOutcome.OK)
                        timeout?.cancel()
                        emit(face)
                    }
                    finish(HostEventOutcome.ERROR)
                } catch (cancelled: CancellationException) {
                    finish(HostEventOutcome.ABANDONED)
                    throw cancelled
                } catch (failure: Throwable) {
                    finish(HostEventOutcome.ERROR)
                    throw failure
                } finally {
                    timeout?.cancel()
                }
            }
        }

        private fun finish(outcome: HostEventOutcome) {
            if (finished.compareAndSet(false, true)) report(outcome)
        }

        private fun report(outcome: HostEventOutcome) {
            runCatching {
                capture.capture {
                    val now = timeProvider.nowMillis()
                    originProvider.current().stepEvent(
                        name = EVENT_NAME,
                        outcome = outcome,
                        monotonicMs = now,
                        durationMs = now - requestedAtMillis,
                        sessionId = sessionId(),
                        tags = mapOf("product.id" to productId.value) +
                            warm?.let { mapOf("worker.state" to if (it) "warm" else "cold") }.orEmpty(),
                    )
                }
            }.logFailure("Failed to capture native host Pocket face event")
        }
    }

    companion object {
        const val EVENT_NAME = "host.pocket.face_loaded"

        // Matches the iOS host, so a timeout means the same wait on both.
        val FACE_TIMEOUT = 30.seconds
    }
}
