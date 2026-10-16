package io.paritytech.polkadotapp.feature_products_impl.domain.pocketFacePreview

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RemoteFaceSource
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import javax.inject.Inject

/**
 * Draws a face served from a developer's machine, through the same decoder and the same renderer the
 * Pocket tab uses. Anything else would let the preview agree with a card the app would not draw.
 */
class PocketFacePreviewInteractor @Inject constructor(
    private val remoteFaces: RemoteFaceSource,
) {
    suspend fun loadFace(url: String): Result<JsWidget> = remoteFaces.fetch(url.trim()).map { it.toJsWidget() }
}
