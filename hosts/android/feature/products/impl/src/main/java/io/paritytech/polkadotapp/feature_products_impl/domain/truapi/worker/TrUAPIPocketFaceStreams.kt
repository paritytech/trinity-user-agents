package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketFaceStreams
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.PocketCardSide
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode
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
    override fun renderFaces(key: PocketCardKey, side: PocketCardSide): Flow<RendererNode> = flow {
        // A card whose product publishes no Pocket worker has nothing to stream, and the reference
        // below is what starts one: the pinned card is on the default tab, so taking it would boot a
        // worker for the personhood product every time the tab is opened.
        val definition = publishedCards.findAnswered(key.productId, key.cardId).getOrNull()?.definition
        if (definition == null || (side == PocketCardSide.BACK && definition.back == null)) return@flow

        val runtime = runtimeProvider.runtime()
            .logFailure("TrUAPI runtime unavailable; Pocket face ${key.cardId.value} stays static")
            .getOrElse { return@flow }

        runtime.acquireWorker(key.productId)
        try {
            emitAll(
                workers.execution(key.productId)
                    .filterNotNull()
                    .flatMapLatest { execution -> execution.faces(key.renderContext(side)) }
                    .reopenAfterFailure("Pocket face stream for ${key.cardId.value}"),
            )
        } finally {
            runtime.releaseWorker(key.productId)
        }
    }

    override fun sendAction(key: PocketCardKey, side: PocketCardSide, actionId: String, payload: ByteArray) {
        val execution = workers.currentExecution(key.productId) ?: return
        runCatching { execution.publishRendererAction(HostRendererActionSubscribeItem(key.renderContext(side), actionId, payload)) }
            .logFailure("truapi.renderer.action '$actionId' for ${key.cardId.value}")
    }

    private fun TrUAPIProductExecution.faces(context: RenderContext): Flow<RendererNode> =
        render(ProductRendererRenderRequest(context, payload = ByteArray(0)))
            .retryWhileConnecting()

    private fun PocketCardKey.renderContext(side: PocketCardSide) = when (side) {
        PocketCardSide.FRONT -> RenderContext.PocketCard(cardId.value)
        PocketCardSide.BACK -> RenderContext.PocketCardBack(cardId.value)
    }
}
