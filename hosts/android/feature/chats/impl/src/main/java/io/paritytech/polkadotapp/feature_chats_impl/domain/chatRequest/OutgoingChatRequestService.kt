package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.scale.toScale
import io.paritytech.polkadotapp.common.utils.flatMap
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
import javax.inject.Inject
import javax.inject.Singleton

interface OutgoingChatRequestService {
    /**
     * Sends a chat request to the specified peer.
     *
     * @param contact The contact to send the request to (must have chatKey for encryption).
     * Contact does not have to be present in db.
     * @param welcomeMessage Optional welcome message to include
     * @return The created ChatRequest
     */
    suspend fun sendChatRequest(
        contact: Contact,
        pushToken: TokenContent?,
        welcomeMessage: ChatMessage.Content.RichText?
    ): Result<ChatRequest>
}

@Singleton
class RealOutgoingChatRequestService @Inject constructor(
    private val chatRequestProver: ChatRequestProver,
    private val chatRequestTransport: ChatRequestTransport,
    private val accountRepository: AccountRepository,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    private val identityProofCodec: IdentityProofCodec,
) : OutgoingChatRequestService {
    override suspend fun sendChatRequest(
        contact: Contact,
        pushToken: TokenContent?,
        welcomeMessage: ChatMessage.Content.RichText?
    ): Result<ChatRequest> {
        val contactMetaAccount = accountRepository.getAccountByIdOrThrow(contact.ourMetaAccountId)

        return constructDecryptedChatRequest(
            contactMetaAccount,
            contact,
            pushToken,
            welcomeMessage
        ).flatMap { request ->
            submitChatRequest(request, contact, contactMetaAccount).map {
                request.toChatRequest()
            }
        }
    }

    private fun ChatRequestDecrypted.toChatRequest(): ChatRequest {
        return ChatRequest(
            welcomeMessageId = message.messageId,
            timestamp = message.timestamp.toLong(),
            direction = ChatRequest.Direction.OUTGOING,
            status = ChatRequest.Status.PENDING
        )
    }

    private suspend fun submitChatRequest(
        request: ChatRequestDecrypted,
        contact: Contact,
        signer: MetaAccount,
    ): Result<Unit> {
        val ourAccountId = signer.defaultAccountId()
        val topics = constructChatRequestTopics(contact, ourAccountId)

        return chatRequestTransport.submitChatRequest(
            topics = topics,
            request = request,
            derivationDomain = contact.sharedSecretDerivationDomain,
            statementSigner = signer
        )
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
        contact: Contact,
        pushToken: TokenContent?,
        welcomeMessage: ChatMessage.Content.RichText?
    ): Result<ChatRequestDecrypted> {
        val walletMetaAccount = accountRepository.getWalletAccount()
        val chatRequestContent = if (contact.isMultiDeviceChatSupported(walletMetaAccount)) {
            val identityProof = identityProofCodec.produce(
                statementAccountId = identityAccount.defaultAccountId(),
                peerIdentityChatPubKey = contact.chatKey,
            )
            VersionedRequestContent.V2.new(
                identityProof = identityProof.toScale(),
                deviceEncPubKey = ourDeviceKeypairProvider.publicKey().toScale(),
                pushToken = pushToken,
                welcomeMessage = welcomeMessage?.toRemote()
            )
        } else {
            VersionedRequestContent.V1.new(
                pushToken = pushToken,
                welcomeMessage = welcomeMessage?.toRemote()
            )
        }

        val requestMessage = ChatRequestMessage.new(chatRequestContent)

        return chatRequestProver.createProof(requestMessage, identityAccount, contact.accountId)
            .map { proof -> ChatRequestDecrypted(requestMessage, proof) }
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
