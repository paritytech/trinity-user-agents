package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runPolling
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasPendingOutgoingRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.signerAccountId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTopic
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTransport
import kotlinx.coroutines.coroutineScope
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds

/**
 * Service for polling topic3 (session topic) for contacts with pending outgoing requests.
 *
 * When we send a chat request, the peer might respond by sending their own request
 * to our topic3. This service detects those responses and auto-accepts mutual requests.
 */
interface IncomingChatRequestService {
    /**
     * Polls for responses on the session topic for all contacts with pending outgoing requests.
     */
    suspend fun pollForResponses(
        metaAccount: MetaAccount,
    )
}

@Singleton
class RealIncomingChatRequestService @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatRequestRepository: ChatRequestRepository,
    private val chatRequestTransport: ChatRequestTransport,
    private val processor: IncomingChatRequestProcessor,
) : IncomingChatRequestService {
    companion object {
        private val POLLING_FREQUENCY = 5.seconds
    }

    override suspend fun pollForResponses(
        metaAccount: MetaAccount,
    ) = coroutineScope {
        logForAccount("Started polling incoming request by session topic", metaAccount)

        runPolling(POLLING_FREQUENCY) {
            val contactsToPoll = contactsRepository.getContactsWithChatRequests(metaAccount.id)
                .filter { it.hasPendingOutgoingRequest() }

            if (contactsToPoll.isNotEmpty()) {
                logForAccount("Performing polling round by session topic for ${contactsToPoll.size} accounts", metaAccount)
            }

            contactsToPoll.forEach { contact ->
                pollForContactResponse(contact.contact, metaAccount)
                    .onFailure {
                        warnForAccount("Failed to poll for response from contact ${contact.contact.accountId}", metaAccount, it)
                    }
            }
        }
    }

    private suspend fun pollForContactResponse(
        contact: Contact,
        metaAccount: MetaAccount,
    ): Result<Unit> = runCatching {
        val sessionTopic = ChatRequestTopic.Session(
            peerAccountId = contact.accountId,
            peerChatKey = contact.chatKey,
            pin = contact.pin,
            ourAccountId = metaAccount.defaultAccountId(),
            direction = ChatRequestTopic.Session.Direction.FROM_PEER
        )

        chatRequestTransport.fetchChatRequests(sessionTopic, contact.sharedSecretDerivationDomain)
            .mapCatching { requests ->
                processRequests(requests, metaAccount, contact)
            }
    }

    private suspend fun processRequests(
        requests: List<ChatRequestDecrypted>,
        metaAccount: MetaAccount,
        contact: Contact,
    ) {
        for (request in requests) {
            if (request.proof.signerAccountId() != contact.accountId) {
                Timber.w(
                    """
                    Detected chat request from unauthorized account.
                    Expected account: ${contact.accountId}
                    Actual account: ${request.proof.signerAccountId()}
                    """.trimIndent()
                )
            }

            val requestData = IncomingChatRequestData(request, metaAccount.id, contact.sharedSecretDerivationDomain)

            processor.processIncomingRequestForOutgoingRequest(requestData, contact)
                .logFailure("Failed to process request: $request")
        }
    }

    private fun logForAccount(log: String, account: MetaAccount) {
        Timber.d("$log for account ${account.name} (${account.purpose})")
    }

    private fun warnForAccount(log: String, account: MetaAccount, throwable: Throwable) {
        Timber.w(throwable, "$log for account ${account.name} (${account.purpose})")
    }
}
