package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId

/**
 * A card a worker manifest publishes; [preview] says where its face tree is read from, and
 * [startsCovered] whether the card opens covered by the product's page.
 */
data class PocketCardDefinition(
    val id: PocketCardId,
    val title: String,
    val preview: PocketCardPreview,
    val startsCovered: Boolean,
)
