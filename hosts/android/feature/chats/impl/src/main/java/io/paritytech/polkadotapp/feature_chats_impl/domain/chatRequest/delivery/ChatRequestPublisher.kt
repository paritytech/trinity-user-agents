package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.Counter
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.OutgoingChatRequestNotPending
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementMaxSizeProvider
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/** Puts a recorded outgoing chat request on the statement store and records how it got there. */
@Singleton
class ChatRequestPublisher @Inject constructor(
    private val outgoingChatRequestService: OutgoingChatRequestService,
    private val chatRequestRepository: ChatRequestRepository,
    private val contactsRepository: ContactsRepository,
    private val chatEngine: ChatEngine,
    private val notificationStatementMaxSizeProvider: NotificationStatementMaxSizeProvider,
) {
    private val activePublishes = Counter<ChatRequestId>()

    suspend fun fitsNotificationStatement(request: ChatRequest, payload: OutgoingChatRequestPayload): Result<Boolean> {
        return notificationStatementMaxSizeProvider.maxSize().flatMap { maxSize ->
            outgoingChatRequestService.fitsStatementSize(request, payload, maxSize)
                .onSuccess { fits -> Timber.d("chatRequestPublisher: request ${request.id} fits ${maxSize.inWholeBytes} bytes: $fits") }
        }
    }

    suspend fun publishFirstDelivery(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        signer: ChatRequestDeliverySigner,
    ): Result<Unit> {
        return submitWhilePending(request, payload, signer) {
            contactsRepository.withTransaction {
                chatRequestRepository.updateDelivery(request.id, signer.delivery)
                chatEngine.updateMessageStatus(request.id, ChatMessage.Status.IS_SENT)
            }
        }
    }

    suspend fun republish(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        signer: ChatRequestDeliverySigner,
    ): Result<Unit> {
        return submitWhilePending(request, payload, signer) {
            chatRequestRepository.updateDelivery(request.id, signer.delivery)
        }
    }

    suspend fun markUndeliverable(request: ChatRequest): Result<Unit> {
        Timber.w("Chat request ${request.id} exceeds the notification statement max size; marking undeliverable")
        return markFailed(request)
    }

    suspend fun markFailed(request: ChatRequest): Result<Unit> = runCancellableCatching {
        contactsRepository.withTransaction {
            chatRequestRepository.updateDelivery(request.id, ChatRequest.Delivery.Failed)
            chatEngine.updateMessageStatus(request.id, ChatMessage.Status.DELIVERY_FAILED)
        }
    }

    suspend fun awaitPublishing(requestId: ChatRequestId) {
        activePublishes.awaitZero(requestId)
    }

    private suspend fun submitWhilePending(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        signer: ChatRequestDeliverySigner,
        recordDelivery: suspend () -> Unit,
    ): Result<Unit> {
        activePublishes.increment(request.id)
        try {
            return ensurePending(request.id)
                .flatMap { submit(request, payload, signer) }
                .flatMap { runCancellableCatching { recordDelivery() } }
        } finally {
            activePublishes.decrement(request.id)
        }
    }

    private suspend fun ensurePending(requestId: ChatRequestId): Result<Unit> {
        return runCancellableCatching { chatRequestRepository.getById(requestId)?.status }.flatMap { status ->
            if (status == ChatRequest.Status.PENDING) Result.success(Unit) else Result.failure(OutgoingChatRequestNotPending(requestId))
        }
    }

    private suspend fun submit(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        signer: ChatRequestDeliverySigner,
    ): Result<Unit> {
        Timber.i("chatRequestPublisher: submitting request ${request.id} as ${signer.delivery}")

        return outgoingChatRequestService.deliverChatRequest(request, payload, signer.prover)
    }
}
