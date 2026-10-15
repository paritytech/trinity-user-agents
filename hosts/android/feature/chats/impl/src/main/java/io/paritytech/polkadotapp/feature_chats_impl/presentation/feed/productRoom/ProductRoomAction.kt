package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

/** What the user does to the parts of a product room that its product declares. */
sealed interface ProductRoomAction {
    /** The user confirmed the link of a product action. */
    data class OpenLink(val url: String) : ProductRoomAction
}
