package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.cancel

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.pendingChatRequestIdOrThrow
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getByIdOrThrow
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getContactOrThrow
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryKeypairDerivation
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigner
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliverySigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.StoredChatRequestCopy
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.OutgoingChatRequestNotPending
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer.ContactChatSigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.DeleteLocalChatUseCase
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementAccountAllocator
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

private const val CANCEL_CONNECTION_TAG = "ChatRequestCancel"

@Singleton
class OutgoingChatRequestCanceller @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatRequestRepository: ChatRequestRepository,
    private val deleteLocalChat: DeleteLocalChatUseCase,
    private val chatSigners: ContactChatSigners,
    private val publisher: ChatRequestPublisher,
    private val outgoingChatRequestService: OutgoingChatRequestService,
    private val keypairDerivation: ChatRequestDeliveryKeypairDerivation,
    private val signers: ChatRequestDeliverySigners,
    private val allocator: NotificationStatementAccountAllocator,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
    private val coroutineDispatchers: CoroutineDispatchers,
) : AppInitializer {
    private val cancelledRequests = Channel<CancelledRequest>(Channel.UNLIMITED)

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        cancelledRequests.receiveAsFlow()
            .onEach { cancelled -> scope.launch(coroutineDispatchers.io) { finishCancel(cancelled) } }
            .launchIn(scope)
    }

    suspend fun cancel(contactAccountId: AccountId): Result<Unit> {
        return runCancellableCatching {
            val contact = contactsRepository.getContactOrThrow(contactAccountId)
            contact to chatRequestRepository.getByIdOrThrow(contact.pendingChatRequestIdOrThrow())
        }.flatMap { (contact, request) ->
            markCancelledAndRemoveChat(contact, request.id)
                .onSuccess { cancelledRequests.trySend(CancelledRequest(contact, request)) }
        }
    }

    private suspend fun finishCancel(cancelled: CancelledRequest) {
        releaseChatSigner(cancelled.contact, cancelled.request.id)
        publishCancelInBackground(cancelled.contact, cancelled.request)
    }

    private suspend fun markCancelledAndRemoveChat(contact: Contact, requestId: ChatRequestId): Result<Unit> {
        return runCancellableCatching {
            contactsRepository.withTransaction {
                val marked = chatRequestRepository.markOutgoingCancelled(requestId)
                if (marked) deleteLocalChat(ChatId.fromContact(contact.accountId))
                marked
            }
        }.flatMap { marked ->
            if (marked) Result.success(Unit) else Result.failure(OutgoingChatRequestNotPending(requestId))
        }
    }

    private suspend fun releaseChatSigner(contact: Contact, requestId: ChatRequestId) {
        chatSigners.release(contact.accountId).logFailure("Failed to release chat signer after request $requestId was cancelled")
    }

    private suspend fun publishCancelInBackground(contact: Contact, request: ChatRequest) {
        chainConnectionRefCounter.withConnectionEnabled(knownChains.people, CANCEL_CONNECTION_TAG) {
            publisher.awaitPublishing(request.id)

            findOwnCopies(contact, request)
                .onSuccess { copies -> cancelCopies(contact, request, copies) }
                .onFailure { error -> Timber.w(error, "chatRequestCancel: copies of request ${request.id} were not fetched; nothing was cancelled") }
        }
    }

    private suspend fun findOwnCopies(contact: Contact, request: ChatRequest): Result<List<OwnRequestCopy>> {
        return outgoingChatRequestService.fetchCopiesToCancel(contact).flatMap { copies ->
            candidateSigners(contact, request).map { candidates ->
                copies.mapNotNull { copy ->
                    val signer = candidates.find { it.accountId == copy.signerAccountId }
                    if (signer == null) Timber.w("chatRequestCancel: skipping a copy of request ${request.id} from an unknown account")

                    signer?.let { OwnRequestCopy(copy, it.prover) }
                }
            }
        }
    }

    private suspend fun candidateSigners(contact: Contact, request: ChatRequest): Result<List<ChatRequestDeliverySigner>> {
        val period = allocator.currentPeriod()
        val oneShotPeriods = if (period > 0u) listOf(period, period - 1u) else listOf(period)
        val oneShotSigners = oneShotPeriods.map { oneShotPeriod ->
            keypairDerivation.deliveryAccount(request.id, oneShotPeriod).map(signers::anonymousSigner)
        }

        return (oneShotSigners + signers.usernameSigner(contact, period)).flattenResult()
    }

    private suspend fun cancelCopies(contact: Contact, request: ChatRequest, copies: List<OwnRequestCopy>) {
        for (copy in copies) {
            outgoingChatRequestService.deliverChatRequestCancel(request, contact, copy.prover, copy.stored.expiry)
                .onSuccess { Timber.i("chatRequestCancel: a copy of request ${request.id} was cancelled") }
                .onFailure { error -> Timber.w(error, "chatRequestCancel: a copy of request ${request.id} was not cancelled") }
        }
    }
}

private class OwnRequestCopy(val stored: StoredChatRequestCopy, val prover: StatementStoreMessageProver)

private class CancelledRequest(val contact: Contact, val request: ChatRequest)
