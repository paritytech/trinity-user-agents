package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel

/** The menu that the product of the room gives for one message (P6.1, 02-DESIGN §6.6). */
sealed interface ProductMessageMenuState {
    val messageId: ChatMessageId

    /** The question to the product is on its way. */
    data class Loading(override val messageId: ChatMessageId) : ProductMessageMenuState

    /**
     * [menu] is null when the room uses the host menu: the product refused, or did not answer in time. [icons] are
     * the image models of the product icons, by action id.
     */
    data class Answered(
        override val messageId: ChatMessageId,
        val menu: ChatMessageMenuModel?,
        val icons: Map<String, Any>,
    ) : ProductMessageMenuState
}
