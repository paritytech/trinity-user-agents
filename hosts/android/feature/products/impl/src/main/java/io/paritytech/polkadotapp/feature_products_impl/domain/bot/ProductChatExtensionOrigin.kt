package io.paritytech.polkadotapp.feature_products_impl.domain.bot

/** Why a product's chat is in the list: the user installed it, or the host placed it there itself. */
enum class ProductChatExtensionOrigin {
    InstalledByUser,
    PlacedByHost,
}
