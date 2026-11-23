package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFace
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFaceSource
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.hasMaterials
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flow
import timber.log.Timber
import uniffi.truapi.RendererNode
import javax.inject.Inject

/** Shows the kept face at once, then every live face the product draws. The core keeps the newest. */
class RealPocketFaceSource @Inject constructor(
    private val store: PocketCardStore,
    private val streams: PocketFaceStreams,
    private val images: ProductImageResolver,
) : PocketFaceSource {
    // Shared into a ViewModel scope that has no handler for a throw, so a failure ends the stream quietly.
    override fun observeFace(key: PocketCardKey): Flow<PocketFace> = flow {
        var shown: MaterialsResolution? = null

        // The next tree's materials are resolved before the shown ones close, so materials both share keep their fetch.
        suspend fun show(tree: RendererNode, resolveMaterials: suspend () -> MaterialsResolution?) {
            val materials = if (tree.hasMaterials()) resolveMaterials() else null
            val previous = shown
            shown = materials
            try {
                emit(PocketFace(tree.toJsWidget(), materials))
            } finally {
                previous?.close()
            }
        }
        try {
            cachedFace(key)?.let { tree ->
                // Resolving waits for the runtime to boot, and the card shows plain until then.
                if (tree.hasMaterials()) emit(PocketFace(tree.toJsWidget(), materials = null))
                show(tree) { streams.resolveKeptMaterials(key, tree) }
            }
            streams.renderFaces(key).collect { face -> show(face.tree) { face.resolveMaterials() } }
        } finally {
            shown?.close()
        }
    }
        .catch { Timber.e(it, "pocket: the face stream for %s ended", key.cardId.value) }

    private suspend fun cachedFace(key: PocketCardKey): RendererNode? = runCatching { store.cachedFace(key) }
        .onFailure { Timber.w(it, "pocket: no kept face for %s", key.cardId.value) }
        .getOrNull()

    override fun sendAction(key: PocketCardKey, actionId: String, payload: ByteArray) =
        streams.sendAction(key, actionId, payload)

    override suspend fun resolveImage(key: PocketCardKey, source: JsImageSource): Result<Uri> =
        images.resolve(key.productId, source)
}
