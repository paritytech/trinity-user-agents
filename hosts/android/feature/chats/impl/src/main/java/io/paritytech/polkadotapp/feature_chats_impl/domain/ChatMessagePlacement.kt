package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

sealed interface ChatMessagePlacement {
    data object Latest : ChatMessagePlacement

    data object ByTimestamp : ChatMessagePlacement

    data class SameAs(val messageId: ChatMessageId) : ChatMessagePlacement
}
