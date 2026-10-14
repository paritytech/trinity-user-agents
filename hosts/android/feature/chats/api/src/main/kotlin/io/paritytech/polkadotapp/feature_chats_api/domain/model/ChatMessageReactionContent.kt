package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp

@JvmInline
value class ChatMessageReactionContent(val emoji: String)

class ChatMessageReaction(
    val messageId: ChatMessageId,
    val content: ChatMessageReactionContent,
    val origin: ChatMessageOrigin,
    val timestamp: Timestamp
)
