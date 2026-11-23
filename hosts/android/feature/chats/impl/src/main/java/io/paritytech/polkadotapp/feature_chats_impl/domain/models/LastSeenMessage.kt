package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

data class LastSeenMessage(
    val id: ChatMessageId,
    val sortOrder: Long,
    val timestamp: Long,
)
