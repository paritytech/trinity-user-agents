package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

data class RecentChat(
    val chatId: ChatId,
    val timestamp: Timestamp,
)
