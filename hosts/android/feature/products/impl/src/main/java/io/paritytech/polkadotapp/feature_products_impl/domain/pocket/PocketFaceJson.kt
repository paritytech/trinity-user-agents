package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import uniffi.truapi.RendererNode
import uniffi.truapi.parseRendererNodeJson

/** A static face from an archive or the app's assets, read by the core so every host agrees on which faces are drawable. */
internal fun readPocketFace(json: String): Result<RendererNode> = runCatching { parseRendererNodeJson(json) }
