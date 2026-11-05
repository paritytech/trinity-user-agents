package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId

/**
 * A card a worker manifest publishes; [preview] says where its face tree is read from, [faceShown]
 * whether the card opens with the face shown above the product's page, and [back] where its back's
 * tree is read from, for a card that can be turned over.
 */
data class PocketCardDefinition(
    val id: PocketCardId,
    val title: String,
    val preview: PocketCardPreview,
    val faceShown: Boolean,
    val back: PocketCardPreview? = null,
)
