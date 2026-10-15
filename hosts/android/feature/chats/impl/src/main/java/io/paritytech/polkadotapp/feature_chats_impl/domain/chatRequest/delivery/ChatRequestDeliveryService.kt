package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestPayload
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

private const val DELIVERY_CONNECTION_TAG = "ChatRequestDelivery"
private const val MAX_BACKOFF_DOUBLINGS = 8
private val FIRST_RETRY_DELAY = 2.seconds
private val MAX_RETRY_DELAY = 5.minutes

/** First delivery of recorded outgoing chat requests, one coroutine per request, retried until it lands. */
@Singleton
class ChatRequestDeliveryService @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatRequestRepository: ChatRequestRepository,
    private val payloadLoader: ChatRequestPayloadLoader,
    private val publisher: ChatRequestPublisher,
    private val accountResolver: ChatRequestDeliveryAccountResolver,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
    private val coroutineDispatchers: CoroutineDispatchers,
) : AppInitializer {
    private val inFlight: MutableSet<ChatRequestId> = ConcurrentHashMap.newKeySet()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        contactsRepository.subscribeContactsWithUndeliveredOutgoingRequests()
            .onEach { pending -> pending.forEach { launchDelivery(it) } }
            .launchIn(scope)
    }

    context(scope: ComputationalScope)
    private fun launchDelivery(pending: ContactWithChatRequest) {
        val request = pending.pendingChatRequest ?: return
        if (!inFlight.add(request.id)) return
        Timber.i("chatRequestDelivery: starting first delivery of request ${request.id}")

        scope.launch(coroutineDispatchers.computation) {
            try {
                chainConnectionRefCounter.withConnectionEnabled(knownChains.people, DELIVERY_CONNECTION_TAG) {
                    deliverUntilDone(pending.contact, request)
                }
            } finally {
                inFlight.remove(request.id)
            }
        }
    }

    private suspend fun deliverUntilDone(contact: Contact, request: ChatRequest) {
        var attempt = 0

        while (isStillAwaitingDelivery(contact, request) && deliver(contact, request).isFailure) {
            attempt++
            val retryIn = retryDelay(attempt)
            Timber.i("chatRequestDelivery: request ${request.id} attempt $attempt failed; retrying in $retryIn")
            delay(retryIn)
        }
        Timber.i("chatRequestDelivery: finished with request ${request.id} after ${attempt + 1} attempts")
    }

    // The peer's own request may have been auto-accepted, or the contact removed, while we were retrying.
    private suspend fun isStillAwaitingDelivery(contact: Contact, request: ChatRequest): Boolean {
        val current = chatRequestRepository.getById(request.id) ?: return false
        val linkedRequestId = contactsRepository.getContact(contact.accountId)?.pendingChatRequestId

        val awaiting = current.delivery == ChatRequest.Delivery.Undelivered &&
            current.status == ChatRequest.Status.PENDING &&
            linkedRequestId == request.id
        if (!awaiting) {
            Timber.i("chatRequestDelivery: request ${request.id} no longer awaits delivery (delivery=${current.delivery}, status=${current.status}, linked=${linkedRequestId == request.id})")
        }

        return awaiting
    }

    private suspend fun deliver(contact: Contact, request: ChatRequest): Result<Unit> {
        return payloadLoader.load(contact, request)
            .flatMap { payload -> deliverPayload(contact, request, payload) }
            .logFailure("Chat request ${request.id} delivery attempt failed")
    }

    private suspend fun deliverPayload(
        contact: Contact,
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
    ): Result<Unit> {
        return publisher.fitsNotificationStatement(request, payload).flatMap { fits ->
            if (!fits) return@flatMap publisher.markUndeliverable(request)

            accountResolver.resolveFirstDelivery(contact, request)
                .flatMap { signer -> publisher.publishFirstDelivery(request, payload, signer) }
                .onSuccess { Timber.i("chatRequestDelivery: request ${request.id} delivered") }
        }
    }

    private fun retryDelay(attempt: Int): Duration {
        val exponential = FIRST_RETRY_DELAY * (1 shl attempt.coerceAtMost(MAX_BACKOFF_DOUBLINGS))
        return exponential.coerceAtMost(MAX_RETRY_DELAY)
    }
}
