package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.renderer.toJsWidget
import uniffi.truapi.parseRendererNodeJson
import javax.inject.Inject

/**
 * A static face from an archive or the app's assets, read by the core and drawn through the same
 * mapping a live face takes, so every host agrees on which faces are drawable.
 */
class PocketFaceJsonDecoder @Inject constructor() {
    fun decode(faceJson: String): Result<JsWidget> = runCatching { parseRendererNodeJson(faceJson) }.map { it.toJsWidget() }
}
