package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/** Where the chat product shows the user's person chats (HX2). */
interface PersonChatRooms {
    /**
     * The room the host names when it gives the chat product a contact to open (`open_link`); null before the network
     * is known. The product answers with the room of the chat with the contact.
     */
    fun homeRoom(): ChatId?

    /**
     * True for a room of the chat product. The host draws it as a person chat, so before the product declares the
     * contact of the room it shows no input, menu or call of the product.
     */
    fun isPersonChatRoom(chatId: ChatId): Boolean = false

    /** True for the chat extension of the chat product, whose rooms are the person chats. */
    fun isPersonChatExtension(extensionId: ChatExtensionId): Boolean = false
}
