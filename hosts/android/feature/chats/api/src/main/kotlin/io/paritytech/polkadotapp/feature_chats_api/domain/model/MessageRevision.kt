package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp

data class MessageRevision(
    val messageId: ChatMessageId,
    val content: ChatMessage.Content,
    val chatId: ChatId,
    val timestamp: Timestamp
)
