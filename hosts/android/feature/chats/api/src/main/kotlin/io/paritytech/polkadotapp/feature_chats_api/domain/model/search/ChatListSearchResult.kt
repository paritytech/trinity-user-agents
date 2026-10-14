package io.paritytech.polkadotapp.feature_chats_api.domain.model.search

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

sealed interface ChatListSearchResult {
    val id: String
    val title: String

    data class Chat(
        override val id: String,
        override val title: String,
        val chatId: ChatId,
    ) : ChatListSearchResult

    data class Message(
        override val id: String,
        override val title: String,
        val chatId: ChatId,
        val messageId: ChatMessageId,
        val snippet: String,
        val timestamp: Timestamp,
    ) : ChatListSearchResult

    data class App(
        override val id: String,
        override val title: String,
        val providerId: ChatExtensionId,
    ) : ChatListSearchResult
}
