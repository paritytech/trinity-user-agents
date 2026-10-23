package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

sealed class StartChatData {
    /** The room of the chat product with the account. */
    data class ProductRoom(val chatId: ChatId) : StartChatData()
}
