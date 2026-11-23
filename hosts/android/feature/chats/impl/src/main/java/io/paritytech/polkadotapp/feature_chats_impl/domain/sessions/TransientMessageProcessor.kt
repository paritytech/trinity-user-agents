package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSeenRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.LastSeenMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.TransientMessage
import javax.inject.Inject

class TransientMessageProcessor @Inject constructor(
    private val chatSeenRepository: ChatSeenRepository,
) {
    suspend fun processReceived(contact: Contact, messages: List<TransientMessage>) {
        if (contact.isBlocked) return

        val chatId = ChatId.fromContact(contact.accountId)
        for (message in messages) {
            when (message) {
                is TransientMessage.Seen -> runCancellableCatching { applyReceivedSeen(chatId, message) }
                    .logFailure("Failed to save a seen message from ${contact.accountId}")
            }
        }
    }

    suspend fun processDelivered(contactAccountId: AccountId, messages: List<TransientMessage>) {
        val chatId = ChatId.fromContact(contactAccountId)
        for (message in messages) {
            when (message) {
                is TransientMessage.Seen -> runCancellableCatching { recordDeliveredSeen(chatId, message) }
                    .logFailure("Failed to save a delivered seen message to $contactAccountId")
            }
        }
    }

    private suspend fun applyReceivedSeen(chatId: ChatId, seen: TransientMessage.Seen) {
        val order = chatSeenRepository.getOutgoingOrder(chatId, seen.messageId)
            ?: chatSeenRepository.getHighestSentOutgoingOrderUpTo(chatId, seen.timestamp)
            ?: return

        chatSeenRepository.raisePeerSeenOrder(chatId, order)
    }

    private suspend fun recordDeliveredSeen(chatId: ChatId, seen: TransientMessage.Seen) {
        val order = chatSeenRepository.getIncomingOrder(chatId, seen.messageId) ?: return

        val lastSentSeen = LastSeenMessage(id = seen.messageId, sortOrder = order, timestamp = seen.timestamp)
        chatSeenRepository.raiseLastSentSeen(chatId, lastSentSeen)
    }
}
