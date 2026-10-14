package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toUrl

/** The label a product declares for one of its cards, unique within that product. */
@JvmInline
value class PocketCardId(val value: String)

data class PocketCardKey(
    val productId: ProductId,
    val cardId: PocketCardId,
) {
    /** The product's launch URL with the card named in its query, so the product opens on this card. */
    fun launchUrl(): String = "${productId.toUrl()}?card=${Uri.encode(cardId.value)}"
}

/** One card in the host's Pocket collection. A [privileged] card is host-placed and removable by nobody. */
data class PocketCard(
    val key: PocketCardKey,
    val title: String,
    val privileged: Boolean,
)
