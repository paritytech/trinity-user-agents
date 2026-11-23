package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.MaterialsResolution
import io.parity.truapi.TrUAPIWorkerExecution
import io.parity.truapi.WorkerExecutionState
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketFaceStreams
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketPublishError
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PublishedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RenderedFace
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.HostRendererActionSubscribeItem
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
    private val publishedCards: PublishedPocketCards,
) : PocketFaceStreams {
    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderFaces(key: PocketCardKey): Flow<RenderedFace> = flow {
        // A card whose product publishes no Pocket worker has nothing to stream, and the reference
        // below is what starts one: the pinned card is on the default tab, so taking it would boot a
        // worker for the personhood product every time the tab is opened.
        if (!publishedCards.isStreamable(key)) return@flow

        val runtime = runtimeProvider.runtime()
            .logFailure("TrUAPI runtime unavailable; Pocket face ${key.cardId.value} stays static")
            .getOrElse { return@flow }

        val claim = runtime.claimWorker(key.productId)
        try {
            emitAll(
                runtime.workerExecutionState(key.productId)
                    .flatMapLatest { state -> (state as? WorkerExecutionState.Running)?.execution?.faces(key) ?: emptyFlow() }
                    .reopenAfterFailure("Pocket face stream for ${key.cardId.value}"),
            )
        } finally {
            claim.release()
        }
    }

    private suspend fun PublishedPocketCards.isStreamable(key: PocketCardKey): Boolean {
        var attempt = 0L
        while (true) {
            val failure = find(key.productId, key.cardId)
                .logFailure("Pocket face ${key.cardId.value} has no published card; it keeps the face it has")
                .exceptionOrNull() ?: return true
            // Only the manifest's own answer is final; a read that did not land says nothing about the card.
            if (failure is PocketPublishError || failure is ProductResolutionError.MalformedManifest) return false
            delay(reopenBackoff(attempt++))
        }
    }

    override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
        val execution = runtimeProvider.currentRuntime()?.currentWorkerExecution(key.productId) ?: return
        runCatching { execution.publishRendererAction(HostRendererActionSubscribeItem(key.renderContext(), actionId, payload)) }
            .logFailure("truapi.renderer.action '$actionId' for ${key.cardId.value}")
    }

    override suspend fun resolveKeptMaterials(key: PocketCardKey, tree: RendererNode): MaterialsResolution? =
        runtimeProvider.runtime()
            .mapCatching { it.resolveMaterials(tree, key.renderContext(), FLOAT_FILTERABLE) }
            .logFailure("Pocket face ${key.cardId.value}: materials unresolved on the runtime")
            .getOrNull()

    private fun TrUAPIWorkerExecution.faces(key: PocketCardKey): Flow<RenderedFace> =
        render(ProductRendererRenderRequest(key.renderContext(), payload = ByteArray(0)))
            .retryWhileConnecting()
            .map { tree -> RenderedFace(tree) { resolveDrawn(key, tree) } }

    private fun TrUAPIWorkerExecution.resolveDrawn(key: PocketCardKey, tree: RendererNode): MaterialsResolution? =
        runCatching { resolveMaterials(tree, key.renderContext(), FLOAT_FILTERABLE) }
            .logFailure("Pocket face ${key.cardId.value}: materials unresolved on the execution")
            .getOrNull()

    private fun PocketCardKey.renderContext() = RenderContext.PocketCard(cardId.value)

    private companion object {
        // The core then filters 32-bit float images in the shader, which is correct on every GPU.
        const val FLOAT_FILTERABLE = false
    }
}
