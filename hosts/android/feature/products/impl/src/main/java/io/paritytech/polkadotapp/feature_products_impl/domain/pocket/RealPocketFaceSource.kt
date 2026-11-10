package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFaceSource
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flow
import timber.log.Timber
import uniffi.truapi.PocketCardSide
import uniffi.truapi.RendererNode
import javax.inject.Inject

/** Shows the kept face at once, then every live face the product draws. The core keeps the newest. */
class RealPocketFaceSource @Inject constructor(
    private val store: PocketCardStore,
    private val streams: PocketFaceStreams,
    private val images: ProductImageResolver,
    private val publishedCards: PublishedPocketCards,
    private val previewLoader: PocketPreviewLoader,
) : PocketFaceSource {
    // Shared into a ViewModel scope that has no handler for a throw, so a failure ends the stream quietly.
    override fun observeFace(key: PocketCardKey): Flow<JsWidget> = flow {
        cachedFace(key)?.let { emit(it.toJsWidget()) }
        streams.renderFaces(key).collect { face -> emit(face.toJsWidget()) }
    }
        .catch { Timber.e(it, "pocket: the face stream for %s ended", key.cardId.value) }

    override fun observeBack(key: PocketCardKey): Flow<JsWidget?> = flow {
        val kept = cachedFace(key, PocketCardSide.BACK)?.also { emit(it.toJsWidget()) }
        val published = publishedCards.findAnswered(key.productId, key.cardId).getOrNull() ?: return@flow
        val shipped = published.definition.back
        if (shipped == null) {
            emit(null)
            return@flow
        }
        if (kept == null) {
            previewLoader.load(key.productId, shipped)
                .logFailure("pocket: no shipped back for ${key.cardId.value}")
                .onSuccess { emit(it.toJsWidget()) }
        }
        streams.renderFaces(key, PocketCardSide.BACK).collect { back -> emit(back.toJsWidget()) }
    }
        .catch { Timber.e(it, "pocket: the back stream for %s ended", key.cardId.value) }

    private suspend fun cachedFace(key: PocketCardKey, side: PocketCardSide = PocketCardSide.FRONT): RendererNode? =
        runCatching { store.cachedFace(key, side) }
            .onFailure { Timber.w(it, "pocket: no kept face for %s", key.cardId.value) }
            .getOrNull()

    override fun sendAction(key: PocketCardKey, side: PocketCardSide, actionId: String, payload: ByteArray) =
        streams.sendAction(key, side, actionId, payload)

    override suspend fun resolveImage(key: PocketCardKey, source: JsImageSource): Result<Uri> =
        images.resolve(key.productId, source)
}
