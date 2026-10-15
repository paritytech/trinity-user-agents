package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.scale.toScale
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getAccountByIdOrThrow
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_api.domain.isMultiDeviceChatSupported
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.IdentityProof
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestProver
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.IdentityProofScale
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.VersionedRequestContent
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTopic
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTransport
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.OutgoingChatRequestTopics
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.RichTextContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenContent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

/** What an outgoing chat request carries besides its id and timestamp. */
class OutgoingChatRequestPayload(
    val contact: Contact,
    val pushToken: TokenContent?,
    val welcomeMessage: ChatMessage.Content.RichText?,
)

fun newOutgoingChatRequest(delivery: ChatRequest.Delivery): ChatRequest {
    return ChatRequest(
        welcomeMessageId = UUID.randomUUID().toString(),
        timestamp = System.currentTimeMillis(),
        direction = ChatRequest.Direction.OUTGOING,
        status = ChatRequest.Status.PENDING,
        delivery = delivery,
    )
}

interface OutgoingChatRequestService {
    /**
     * Creates a new chat request and submits it signed by the contact's own meta account.
     * [OutgoingChatRequestPayload.contact] does not have to be present in db.
     */
    suspend fun sendChatRequest(payload: OutgoingChatRequestPayload): Result<ChatRequest>

    /**
     * Submits the already recorded [request]. Its inner proof stays with the contact's meta account; only the
     * outer statement is signed by [statementProver], which is what the statement store shows publicly.
     */
    suspend fun deliverChatRequest(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        statementProver: StatementStoreMessageProver,
    ): Result<Unit>

    suspend fun fitsStatementSize(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        limit: InformationSize,
    ): Result<Boolean>

    suspend fun isStoredBy(contact: Contact, signer: AccountId): Result<Boolean>
}

@Singleton
class RealOutgoingChatRequestService @Inject constructor(
    private val chatRequestProver: ChatRequestProver,
    private val chatRequestTransport: ChatRequestTransport,
    private val accountRepository: AccountRepository,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    private val identityProofCodec: IdentityProofCodec,
    private val statementProverFactory: StatementStoreMessageProver.Factory,
) : OutgoingChatRequestService {
    override suspend fun sendChatRequest(payload: OutgoingChatRequestPayload): Result<ChatRequest> {
        val request = newOutgoingChatRequest(ChatRequest.Delivery.Delivered)

        return identityAccountOf(payload.contact)
            .flatMap { identity -> deliverChatRequest(request, payload, statementProverFactory.createKeyPairProver(identity)) }
            .map { request }
    }

    override suspend fun deliverChatRequest(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        statementProver: StatementStoreMessageProver,
    ): Result<Unit> {
        return composeRequest(request, payload).flatMap { composed ->
            chatRequestTransport.submitChatRequest(
                topics = composed.topics,
                request = composed.decrypted,
                derivationDomain = payload.contact.sharedSecretDerivationDomain,
                statementProver = statementProver,
            )
        }
    }

    override suspend fun fitsStatementSize(
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
        limit: InformationSize,
    ): Result<Boolean> {
        return composeRequest(request, payload)
            .flatMap { composed -> chatRequestTransport.estimateStatementSize(composed.topics, composed.decrypted) }
            .map { size -> size <= limit }
    }

    override suspend fun isStoredBy(contact: Contact, signer: AccountId): Result<Boolean> {
        return identityAccountOf(contact).flatMap { identityAccount ->
            val session = constructChatRequestTopics(contact, identityAccount.defaultAccountId()).session
            chatRequestTransport.isChatRequestStored(session, contact.sharedSecretDerivationDomain, signer)
        }
    }

    private suspend fun composeRequest(request: ChatRequest, payload: OutgoingChatRequestPayload): Result<ComposedChatRequest> {
        return identityAccountOf(payload.contact).flatMap { identityAccount ->
            val topics = constructChatRequestTopics(payload.contact, identityAccount.defaultAccountId())
            constructDecryptedChatRequest(identityAccount, request, payload)
                .map { decrypted -> ComposedChatRequest(topics, decrypted) }
        }
    }

    private suspend fun identityAccountOf(contact: Contact): Result<MetaAccount> = runCancellableCatching {
        accountRepository.getAccountByIdOrThrow(contact.ourMetaAccountId)
    }

    private fun constructChatRequestTopics(
        contact: Contact,
        ourAccountId: AccountId
    ): OutgoingChatRequestTopics {
        val acceptor = contact.accountId
        val currentDay = ChatRequestTopicDerivation.getCurrentDay()
        return OutgoingChatRequestTopics(
            full = ChatRequestTopic.Full(acceptor),
            day = ChatRequestTopic.Day(acceptor, currentDay),
            session = ChatRequestTopic.Session(
                peerAccountId = contact.accountId,
                peerChatKey = contact.chatKey,
                pin = contact.pin,
                ourAccountId = ourAccountId,
                direction = ChatRequestTopic.Session.Direction.TO_PEER
            )
        )
    }

    private suspend fun constructDecryptedChatRequest(
        identityAccount: MetaAccount,
        request: ChatRequest,
        payload: OutgoingChatRequestPayload,
    ): Result<ChatRequestDecrypted> {
        val requestMessage = ChatRequestMessage(
            messageId = request.id,
            timestamp = request.timestamp.toULong(),
            content = constructRequestContent(identityAccount, payload),
        )

        return chatRequestProver.createProof(requestMessage, identityAccount, payload.contact.accountId)
            .map { proof -> ChatRequestDecrypted(requestMessage, proof) }
    }

    private suspend fun constructRequestContent(
        identityAccount: MetaAccount,
        payload: OutgoingChatRequestPayload,
    ): VersionedRequestContent {
        val welcomeMessage = payload.welcomeMessage?.toRemote()
        if (!payload.contact.isMultiDeviceChatSupported(accountRepository.getWalletAccount())) {
            return VersionedRequestContent.V1.new(pushToken = payload.pushToken, welcomeMessage = welcomeMessage)
        }

        val identityProof = identityProofCodec.produce(
            statementAccountId = identityAccount.defaultAccountId(),
            peerIdentityChatPubKey = payload.contact.chatKey,
        )
        return VersionedRequestContent.V2.new(
            identityProof = identityProof.toScale(),
            deviceEncPubKey = ourDeviceKeypairProvider.publicKey().toScale(),
            pushToken = payload.pushToken,
            welcomeMessage = welcomeMessage,
        )
    }

    private fun IdentityProof.toScale(): IdentityProofScale {
        return IdentityProofScale(
            identityAccountId = identityAccountId.value,
            proof = proof.value,
        )
    }

    private fun ChatMessage.Content.RichText.toRemote(): RichTextContent {
        return RichTextContent(
            text = text,
            attachments = null
        )
    }
}

private class ComposedChatRequest(
    val topics: OutgoingChatRequestTopics,
    val decrypted: ChatRequestDecrypted,
)
