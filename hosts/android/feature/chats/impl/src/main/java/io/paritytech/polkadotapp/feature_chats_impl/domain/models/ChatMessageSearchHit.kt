package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

data class ChatMessageSearchHit(
    val chatId: ChatId,
    val messageId: ChatMessageId,
    val snippet: String,
    val timestamp: Timestamp,
)
