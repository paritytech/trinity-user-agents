package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketFaceStreams
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.retryWhen
import timber.log.Timber
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

/** A restarted worker gets a fresh render stream while the last face stays visible. */
class TrUAPIPocketFaceStreams @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val workers: TrUAPIWorkerSupervisor,
    private val publishedCards: PublishedPocketCards,
) : PocketFaceStreams {
    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderFaces(key: PocketCardKey): Flow<JsWidget> = flow {
        if (!publishedCards.isStreamable(key)) return@flow

        runtimeProvider.runtime()
            .logFailure("TrUAPI runtime unavailable; Pocket face ${key.cardId.value} stays static")
            .getOrElse { return@flow }

        emitAll(
            workers.execution(key.productId)
                .filterNotNull()
                .flatMapLatest { execution -> execution.faces(key) }
                .retryAfterStreamEnd(key),
        )
    }

    private suspend fun PublishedPocketCards.isStreamable(key: PocketCardKey): Boolean =
        find(key.productId, key.cardId)
            .logFailure("Pocket face ${key.cardId.value} has no published card; it keeps the face it has")
            .isSuccess

    /**
     * A render that fails is opened again on the same worker. Ending here instead would leave the
     * card static for the worker's whole life: the stream above only opens a new render when a
     * different execution is published, and a stop publishes none.
     */
    private fun <T> Flow<T>.retryAfterStreamEnd(key: PocketCardKey): Flow<T> = retryWhen { failure, attempt ->
        Timber.w(failure, "Pocket face stream for %s ended, reopening", key.cardId.value)
        delay(reopenDelay(attempt))
        true
    }

    private fun reopenDelay(attempt: Long): Duration =
        minOf(REOPEN_DELAY * (1 shl attempt.coerceAtMost(MAX_BACKOFF_SHIFT).toInt()), MAX_REOPEN_DELAY)

    override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
        val execution = workers.currentExecution(key.productId) ?: return
        runCatching { execution.publishRendererAction(HostRendererActionSubscribeItem(key.renderContext(), actionId, payload)) }
            .logFailure("truapi.renderer.action '$actionId' for ${key.cardId.value}")
    }

    private fun TrUAPIProductExecution.faces(key: PocketCardKey): Flow<JsWidget> =
        render(ProductRendererRenderRequest(key.renderContext(), payload = ByteArray(0)))
            .retryWhileConnecting(CONNECT_ATTEMPTS, CONNECT_RETRY_DELAY)
            .map { it.toJsWidget() }

    private fun PocketCardKey.renderContext() = RenderContext.PocketCard(cardId.value)

    private companion object {
        const val CONNECT_ATTEMPTS = 40L
        val CONNECT_RETRY_DELAY = 250.milliseconds

        // Doubling from a second up to half a minute: a worker that is simply slow is picked up at
        // once, and one that is broken is not asked on a loop for as long as its card is on screen.
        val REOPEN_DELAY = 1.seconds
        val MAX_REOPEN_DELAY = 30.seconds
        const val MAX_BACKOFF_SHIFT = 5L
    }
}
