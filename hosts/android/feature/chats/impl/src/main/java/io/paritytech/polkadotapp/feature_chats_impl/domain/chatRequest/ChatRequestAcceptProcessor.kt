package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isPendingOutgoing
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveProcessor
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Processor that auto-accepts outgoing chat requests when receiving a response from the peer.
 *
 * Acceptance conditions:
 * 1. Explicit ChatAccepted message matching our request ID
 * 2. Any message from the contact after our request timestamp (hole punching)
 */
@Singleton
class ChatRequestAcceptProcessor @Inject constructor(
    private val chatRequestRepository: ChatRequestRepository,
    private val contactsRepository: ContactsRepository,
    private val incomingChatRequestProcessor: IncomingChatRequestProcessor,
) : ChatMessageSaveProcessor {
    override suspend fun onMessageSaved(message: ChatMessage) {
        Timber.d("onMessageSaved: ${message.id} (${message.content})")

        // Only process incoming messages from contacts
        val contactVariant = message.chatId.contactOrNull() ?: return
        if (message.origin !is ChatMessageOrigin.Contact) return

        val contact = contactsRepository.getContact(contactVariant.contactAccountId) ?: return
        val requestId = contact.pendingChatRequestId ?: return
        val request = chatRequestRepository.getById(requestId) ?: return

        if (!request.isPendingOutgoing()) return

        Timber.d("Detected pending outgoing request: ${message.id} (${message.content})")

        val shouldAccept = when (val content = message.content) {
            // Condition 1a: V2 acceptance with single accepting device
            is ChatMessage.Content.DeviceChatAccepted -> content.requestId == request.id
            // Condition 1b: Legacy V1 acceptance
            is ChatMessage.Content.ChatAccepted -> content.requestId == request.id
            // Condition 2: Any message after request timestamp (hole punching)
            else -> message.timestamp > request.timestamp
        }

        if (shouldAccept) {
            Timber.d("Marking request as accepted: ${message.id} (${message.content})")

            incomingChatRequestProcessor.markOutgoingRequestAccepted(contact, request)
        }
    }
}
