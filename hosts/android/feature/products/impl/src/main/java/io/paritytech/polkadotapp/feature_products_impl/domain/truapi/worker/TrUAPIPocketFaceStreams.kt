package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.MaterialsResolution
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketFaceStreams
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RenderedFace
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode
import javax.inject.Inject

class TrUAPIPocketFaceStreams @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val workers: TrUAPIWorkerSupervisor,
) : PocketFaceStreams {
    @OptIn(ExperimentalCoroutinesApi::class)
    override fun renderFaces(key: PocketCardKey): Flow<RenderedFace> =
        workers.execution(key.productId)
            .filterNotNull()
            .flatMapLatest { execution -> execution.faces(key) }
            .reopenAfterFailure("Pocket face stream for ${key.cardId.value}")

    override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) {
        val execution = workers.currentExecution(key.productId) ?: return
        runCatching { execution.publishRendererAction(HostRendererActionSubscribeItem(key.renderContext(), actionId, payload)) }
            .logFailure("truapi.renderer.action '$actionId' for ${key.cardId.value}")
    }

    override suspend fun resolveKeptMaterials(key: PocketCardKey, tree: RendererNode): MaterialsResolution? =
        runtimeProvider.runtime()
            .mapCatching { it.resolveMaterials(tree, key.renderContext(), FLOAT_FILTERABLE) }
            .logFailure("Pocket face ${key.cardId.value}: materials unresolved on the runtime")
            .getOrNull()

    private fun TrUAPIProductExecution.faces(key: PocketCardKey): Flow<RenderedFace> =
        render(ProductRendererRenderRequest(key.renderContext(), payload = ByteArray(0)))
            .retryWhileConnecting()
            .map { tree -> RenderedFace(tree) { resolveDrawn(key, tree) } }

    private fun TrUAPIProductExecution.resolveDrawn(key: PocketCardKey, tree: RendererNode): MaterialsResolution? =
        runCatching { resolveMaterials(tree, key.renderContext(), FLOAT_FILTERABLE) }
            .logFailure("Pocket face ${key.cardId.value}: materials unresolved on the execution")
            .getOrNull()

    private fun PocketCardKey.renderContext() = RenderContext.PocketCard(cardId.value)

    private companion object {
        // The core then filters 32-bit float images in the shader, which is correct on every GPU.
        const val FLOAT_FILTERABLE = false
    }
}
