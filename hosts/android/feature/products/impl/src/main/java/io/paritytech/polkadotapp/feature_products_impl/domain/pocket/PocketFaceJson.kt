package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import uniffi.truapi.RendererNode
import uniffi.truapi.parseRendererNodeJson

internal fun readPocketFace(json: String): Result<RendererNode> = runCatching { parseRendererNodeJson(json) }
