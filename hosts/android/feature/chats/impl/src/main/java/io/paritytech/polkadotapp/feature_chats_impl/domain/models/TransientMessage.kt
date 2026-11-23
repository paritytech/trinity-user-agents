package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

sealed interface TransientMessage {
    class Seen(val messageId: ChatMessageId, val timestamp: Long) : TransientMessage
}
