package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatAcceptanceRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.ChatRequestDeliveryAccountResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.DELIVERY_RETRY_DELAY
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery.endsDelivery
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningGate
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.awaitRetry
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

private const val DELIVERY_CONNECTION_TAG = "ChatAcceptanceDelivery"

/** First delivery of recorded acceptances, one coroutine per acceptance, retried until it lands. */
@Singleton
class ChatAcceptanceDeliveryService @Inject constructor(
    private val chatAcceptanceRepository: ChatAcceptanceRepository,
    private val contactsRepository: ContactsRepository,
    private val accountResolver: ChatRequestDeliveryAccountResolver,
    private val publisher: ChatAcceptancePublisher,
    private val identitySigningGate: IdentitySigningGate,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
    private val coroutineDispatchers: CoroutineDispatchers,
) : AppInitializer {
    private val inFlight: MutableSet<ChatMessageId> = ConcurrentHashMap.newKeySet()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCancellableCatching {
        chatAcceptanceRepository.subscribeUndelivered()
            .onEach { pending -> pending.forEach { launchDelivery(it) } }
            .launchIn(scope)
    }

    context(scope: ComputationalScope)
    private fun launchDelivery(acceptance: ChatAcceptance) {
        if (!inFlight.add(acceptance.messageId)) return
        Timber.i("chatAcceptance: starting first delivery of the acceptance of ${acceptance.requestId}")

        scope.launch(coroutineDispatchers.computation) {
            try {
                chainConnectionRefCounter.withConnectionEnabled(knownChains.people, DELIVERY_CONNECTION_TAG) {
                    deliverUntilDone(acceptance)
                }
            } finally {
                inFlight.remove(acceptance.messageId)
            }
        }
    }

    private suspend fun deliverUntilDone(acceptance: ChatAcceptance) {
        var attempt = 0

        while (isStillUndelivered(acceptance)) {
            val outcome = deliver(acceptance)
            if (outcome.isSuccess) break

            val error = outcome.exceptionOrNull()
            if (error?.endsDelivery() == true) {
                Timber.i("chatAcceptance: acceptance of ${acceptance.requestId} fails: ${error.message}")
                publisher.markFailed(acceptance).logFailure("Acceptance of ${acceptance.requestId} could not be marked failed")
                break
            }

            attempt++
            val retryIn = DELIVERY_RETRY_DELAY.delayFor(attempt)
            Timber.i("chatAcceptance: acceptance of ${acceptance.requestId} attempt $attempt failed; retrying in $retryIn")
            identitySigningGate.awaitRetry(acceptance.contactAccountId, error, retryIn)
        }
    }

    // A newer acceptance of the same contact, or the contact's removal, supersedes this one.
    private suspend fun isStillUndelivered(acceptance: ChatAcceptance): Boolean {
        val current = chatAcceptanceRepository.get(acceptance.contactAccountId) ?: return false

        return current.messageId == acceptance.messageId && current.delivery == ChatRequest.Delivery.Undelivered
    }

    private suspend fun deliver(acceptance: ChatAcceptance): Result<Unit> {
        val contact = contactsRepository.getContact(acceptance.contactAccountId)
            ?: return Result.failure(IllegalStateException("No contact ${acceptance.contactAccountId} for its acceptance"))

        return accountResolver.resolveAcceptance(contact, acceptance.requestId)
            .flatMap { signer -> publisher.publishFirstDelivery(contact, acceptance, signer) }
            .onSuccess { Timber.i("chatAcceptance: acceptance of ${acceptance.requestId} delivered") }
            .logFailure("Acceptance of ${acceptance.requestId} delivery attempt failed")
    }
}
