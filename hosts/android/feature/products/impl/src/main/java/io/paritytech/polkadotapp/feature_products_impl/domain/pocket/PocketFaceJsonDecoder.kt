package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import uniffi.truapi.RendererNode
import uniffi.truapi.parseRendererNodeJson
import javax.inject.Inject

/** A static face from an archive or the app's assets, read by the core so every host agrees on which faces are drawable. */
class PocketFaceJsonDecoder @Inject constructor() {
    fun decode(faceJson: String): Result<RendererNode> = runCatching { parseRendererNodeJson(faceJson) }
}
