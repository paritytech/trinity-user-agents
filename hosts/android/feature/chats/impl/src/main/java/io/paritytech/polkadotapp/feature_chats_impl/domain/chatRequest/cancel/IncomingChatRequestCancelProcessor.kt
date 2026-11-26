package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.cancel

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isIncoming
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestStatement
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestVerifier
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer.ContactChatSigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.DeleteLocalChatUseCase
import timber.log.Timber
import javax.inject.Inject

class IncomingChatRequestCancelProcessor @Inject constructor(
    private val verifier: IncomingChatRequestVerifier,
    private val chatRequestRepository: ChatRequestRepository,
    private val contactsRepository: ContactsRepository,
    private val chatMessageRepository: ChatMessageRepository,
    private val deleteLocalChat: DeleteLocalChatUseCase,
    private val deleteRoomUseCase: DeleteRoomUseCase,
    private val chatSigners: ContactChatSigners,
) {
    suspend fun process(cancel: IncomingChatRequestStatement.Cancel): Result<Unit> {
        return verifier.verifyCancel(cancel).flatMap {
            runCancellableCatching { applyCancel(cancel) }
        }
    }

    private suspend fun applyCancel(cancel: IncomingChatRequestStatement.Cancel) {
        val senderAccountId = cancel.identityProof.identityAccountId
        val request = chatRequestRepository.getById(cancel.requestId)
            ?: return rememberCancelledRequest(cancel)

        val removed = request.isIncoming() && cancel.timestamp > request.timestamp && removeRequestChat(request, senderAccountId)
        if (removed) {
            Timber.i("chatRequestCancel: request ${request.id} was cancelled by its sender")
        } else {
            Timber.d("chatRequestCancel: cancel of request ${request.id} does not apply (status=${request.status}, direction=${request.direction})")
        }
    }

    private suspend fun rememberCancelledRequest(cancel: IncomingChatRequestStatement.Cancel) {
        val request = ChatRequest(
            welcomeMessageId = cancel.requestId,
            timestamp = cancel.timestamp,
            direction = ChatRequest.Direction.INCOMING,
            status = ChatRequest.Status.CANCELLED,
            delivery = ChatRequest.Delivery.DeliveredUntracked,
        )
        chatRequestRepository.save(request)

        Timber.i("chatRequestCancel: remembered cancelled request ${cancel.requestId} before its copy arrived")
    }

    private suspend fun removeRequestChat(request: ChatRequest, senderAccountId: AccountId): Boolean {
        return when (request.status) {
            ChatRequest.Status.PENDING -> removePendingRequestChat(request.id, senderAccountId)
            ChatRequest.Status.ACCEPTED -> removeAcceptedRequestChat(request.id, senderAccountId)
            ChatRequest.Status.DECLINED,
            ChatRequest.Status.CANCELLED -> false
        }
    }

    private suspend fun removePendingRequestChat(requestId: ChatRequestId, senderAccountId: AccountId): Boolean {
        return contactsRepository.withTransaction {
            val contact = contactsRepository.getContact(senderAccountId)
            if (contact == null || contact.pendingChatRequestId != requestId || contact.isBlocked) return@withTransaction false

            chatRequestRepository.updateStatus(requestId, ChatRequest.Status.CANCELLED)
            contactsRepository.deleteContact(senderAccountId)
            deleteRoomUseCase.invoke(ChatId.fromContact(senderAccountId))
            true
        }
    }

    private suspend fun removeAcceptedRequestChat(requestId: ChatRequestId, senderAccountId: AccountId): Boolean {
        val chatId = ChatId.fromContact(senderAccountId)

        val removed = contactsRepository.withTransaction {
            if (!chatMessageRepository.hasChatAcceptedMessage(chatId, requestId)) return@withTransaction false
            if (contactsRepository.getContact(senderAccountId)?.isBlocked == true) return@withTransaction false
            if (chatMessageRepository.hasConversationMessages(chatId)) return@withTransaction false

            chatRequestRepository.updateStatus(requestId, ChatRequest.Status.CANCELLED)
            deleteLocalChat(chatId)
            true
        }
        if (removed) {
            chatSigners.release(senderAccountId).logFailure("Failed to release chat signer after request $requestId was cancelled")
        }
        return removed
    }
}
