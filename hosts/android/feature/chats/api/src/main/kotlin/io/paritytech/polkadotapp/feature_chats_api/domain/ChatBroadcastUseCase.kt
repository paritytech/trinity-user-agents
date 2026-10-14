package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact

/**
 * Sends a message to every contact chat via the standard outgoing pipeline.
 *
 * Per-contact failures are logged but do not short-circuit the broadcast.
 */
interface ChatBroadcastUseCase {
    suspend fun broadcastToContacts(content: ChatMessage.Content, contactFilter: (Contact) -> Boolean): Result<Unit>
}
