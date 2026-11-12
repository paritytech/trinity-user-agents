package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import uniffi.truapi.RendererNode

/** A card together with the newest face tree the host holds for it. */
data class CachedPocketCard(
    val card: PocketCard,
    val face: RendererNode,
)
