package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NOTIFICATION_STATEMENT_MAX_SIZE
import timber.log.Timber
import javax.inject.Inject

/** Puts a recorded outgoing chat request on the statement store and records how it got there. */
class ChatRequestPublisher @Inject constructor(
    private val outgoingChatRequestService: OutgoingChatRequestService,
    private val chatRequestRepository: ChatRequestRepository,
    private val contactsRepository: ContactsRepository,
    private val chatEngine: ChatEngine,
) {
    suspend fun fitsNotificationStatement(request: ChatRequest, payload: OutgoingChatRequestPayload): Result<Boolean> {
        return outgoingChatRequestService.fitsStatementSize(request, payload, NOTIFICATION_STATEMENT_MAX_SIZE)
            .onSuccess { fits -> Timber.d("chatRequestPublisher: request ${request.id} fits $NOTIFICATION_STATEMENT_MAX_SIZE: $fits") }
    }

    suspend fun publishFirstDelivery(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        signer: ChatRequestDeliverySigner,
    ): Result<Unit> {
        return submit(request, payload, signer).flatMap {
            runCancellableCatching {
                contactsRepository.withTransaction {
                    chatRequestRepository.updateDelivery(request.id, signer.delivery)
                    chatEngine.updateMessageStatus(request.id, ChatMessage.Status.IS_SENT)
                }
            }
        }
    }

    suspend fun republish(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        signer: ChatRequestDeliverySigner,
    ): Result<Unit> {
        return submit(request, payload, signer).flatMap {
            runCancellableCatching { chatRequestRepository.updateDelivery(request.id, signer.delivery) }
        }
    }

    suspend fun markUndeliverable(request: ChatRequest): Result<Unit> = runCancellableCatching {
        Timber.w("Chat request ${request.id} exceeds $NOTIFICATION_STATEMENT_MAX_SIZE; marking undeliverable")
        contactsRepository.withTransaction {
            chatRequestRepository.updateDelivery(request.id, ChatRequest.Delivery.Failed)
            chatEngine.updateMessageStatus(request.id, ChatMessage.Status.DELIVERY_FAILED)
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
