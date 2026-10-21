package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/** Where the chat product shows the user's person chats, on an install whose chats it runs (HX2). */
interface PersonChatRooms {
    /** The home room of the chat product, where a chat with someone new starts; null before the network is known. */
    fun homeRoom(): ChatId?
}
