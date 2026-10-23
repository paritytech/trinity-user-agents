package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models

import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData

fun StartChatData.toChatFeedPayload(): ChatFeedPayload = when (this) {
    is StartChatData.ProductRoom -> ChatFeedPayload.existingChat(chatId)
}
