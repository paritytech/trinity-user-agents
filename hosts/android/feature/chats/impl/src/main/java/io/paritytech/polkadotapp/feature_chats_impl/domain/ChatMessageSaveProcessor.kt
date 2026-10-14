package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage

/**
 * Interface for processing messages immediately when they're saved.
 * Implementations are called for both incoming and outgoing messages.
 */
interface ChatMessageSaveProcessor {
    suspend fun onMessageSaved(message: ChatMessage)
}
