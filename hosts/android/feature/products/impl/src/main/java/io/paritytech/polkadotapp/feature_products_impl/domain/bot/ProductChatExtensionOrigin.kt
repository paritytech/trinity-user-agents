package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter

/** Why a product's chat is in the list: the user installed it, or the host placed it there itself. */
sealed interface ProductChatExtensionOrigin {
    data object InstalledByUser : ProductChatExtensionOrigin

    /**
     * The host placed [placedRoom], the room the product registers. It is that room the host pins,
     * not the product's other rooms: a product moved into chat brings every room it ever created.
     */
    data class PlacedByHost(val placedRoom: ProductChatIdParameter) : ProductChatExtensionOrigin
}
