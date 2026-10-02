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
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import javax.inject.Inject

/**
 * Faces over the core: one worker reference is taken for the collection, the product's worker is
 * awaited, and `render` is opened on the card's context. A render stream that ends or fails leaves
 * the last face on screen and is opened again; a worker that restarts gets a fresh stream.
 */
class TrUAPIPocketFaceStreams @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val workers: TrUAPIWorkerSupervisor,
    private val publishedCards: PublishedPocketCards,
) : PocketFaceStreams {
    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderFaces(key: PocketCardKey): Flow<JsWidget> = flow {
        // A card whose product publishes no Pocket worker has nothing to stream, and the reference
        // below is what starts one: the pinned card is on the default tab, so taking it would boot a
        // worker for the personhood product every time the tab is opened.
        if (!publishedCards.isStreamable(key)) return@flow

        val runtime = runtimeProvider.runtime()
            .logFailure("TrUAPI runtime unavailable; Pocket face ${key.cardId.value} stays static")
            .getOrElse { return@flow }

        runtime.acquireWorker(key.productId.value)
        try {
            emitAll(
                workers.execution(key.productId)
                    .filterNotNull()
                    .flatMapLatest { execution -> execution.faces(key) }
                    .reopenAfterFailure("Pocket face stream for ${key.cardId.value}"),
            )
        } finally {
            runtime.releaseWorker(key.productId.value)
        }
    }

    private suspend fun PublishedPocketCards.isStreamable(key: PocketCardKey): Boolean =
        find(key.productId, key.cardId)
            .logFailure("Pocket face ${key.cardId.value} has no published card; it keeps the face it has")
            .isSuccess

    override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
        val execution = workers.currentExecution(key.productId) ?: return
        runCatching { execution.publishRendererAction(HostRendererActionSubscribeItem(key.renderContext(), actionId, payload)) }
            .logFailure("truapi.renderer.action '$actionId' for ${key.cardId.value}")
    }

    private fun TrUAPIProductExecution.faces(key: PocketCardKey): Flow<JsWidget> =
        render(ProductRendererRenderRequest(key.renderContext(), payload = ByteArray(0)))
            .retryWhileConnecting()
            .map { it.toJsWidget() }

    private fun PocketCardKey.renderContext() = RenderContext.PocketCard(cardId.value)
}
