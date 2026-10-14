package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.scale.toDomain
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.domain.model.x25519OrNull
import io.paritytech.polkadotapp.common.utils.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.requireConsumerInfo
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.pendingChatRequestIdOrThrow
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.VersionedRequestContent
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.signerAccountId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getByIdOrThrow
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.RichTextContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenPlatform
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Incoming chat request with all necessary data for processing.
 */
data class IncomingChatRequestData(
    val request: ChatRequestDecrypted,
    val ourMetaAccountId: Long,
    val derivationDomain: SharedSecretDerivationDomain,
)

/**
 * Resolves the sender's user-identity sr25519 accountId from a decrypted chat
 * request. For V1 single-device, the inner proof is signed by the identity
 * itself, so the proof signer IS the user identity. For V2 multi-device, the
 * inner proof is signed by the sender's per-device sr25519 (which is never
 * registered in `Resources.Consumers` on chain); the owning user identity is
 * carried separately in the request body as `RequestContentV2.identityAccountId`,
 * and that is what contact resolution + chain lookups must use.
 */
private fun ChatRequestDecrypted.peerIdentityAccountId(): AccountId =
    when (val content = message.content) {
        is VersionedRequestContent.V1 -> proof.signerAccountId()
        is VersionedRequestContent.V2 -> content.content.identityProof.identityAccountId.toDataByteArray()
    }

/**
 * Result of processing an incoming chat request.
 */
sealed class IncomingRequestProcessingResult {
    /** Request was ignored (e.g., already exists with same or newer timestamp) */
    sealed class Ignored : IncomingRequestProcessingResult() {
        data object ChatRequestAlreadyProcessed : Ignored()
    }

    /** New contact and request were created */
    data class NewContactCreated(val contact: Contact, val request: ChatRequest) : IncomingRequestProcessingResult()

    /** Existing request was updated with newer data */
    data class RequestUpdated(val request: ChatRequest) : IncomingRequestProcessingResult()

    /** Auto-accepted because we already sent a request to this peer */
    data class AutoAccepted(val contact: Contact) : IncomingRequestProcessingResult()
}

/**
 * Handles the coordination logic for processing incoming chat requests.
 *
 * Resolution rules:
 * 1. Existing pending outgoing request -> auto-accept (mutual)
 * 2. Existing pending incoming request -> update if newer timestamp
 * 3. Already-seen request id -> ignore (already processed)
 * 4. Established contact re-invites (new request id) -> re-establish / auto-accept (reconnect)
 * 5. No contact -> create new contact with pending request
 */
interface IncomingChatRequestProcessor {
    suspend fun processIncomingRequest(data: IncomingChatRequestData): Result<IncomingRequestProcessingResult>

    suspend fun processIncomingRequestForOutgoingRequest(
        data: IncomingChatRequestData,
        contact: Contact
    ): Result<IncomingRequestProcessingResult.AutoAccepted>

    suspend fun acceptIncomingRequest(contact: Contact): Result<Unit>

    suspend fun declineIncomingRequest(contact: Contact): Result<Unit>

    suspend fun markOutgoingRequestAccepted(contact: Contact, ourRequest: ChatRequest): Result<Unit>
}

@Singleton
class RealIncomingChatRequestProcessor @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatRequestRepository: ChatRequestRepository,
    private val chatRoomRepository: ChatRoomRepository,
    private val resourcesRepository: ResourcesRepository,
    private val verifier: IncomingChatRequestVerifier,
    private val accountRepository: AccountRepository,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    private val chatEngine: dagger.Lazy<ChatEngine>,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
) : IncomingChatRequestProcessor {
    private companion object {
        const val ACCEPTED_MESSAGE_ID_PREFIX = "req-accepted:"
    }

    override suspend fun processIncomingRequest(
        data: IncomingChatRequestData
    ): Result<IncomingRequestProcessingResult> = runCatching {
        verifier.verify(data.request).getOrThrow()
        val senderAccountId = data.request.peerIdentityAccountId()
        val existingContact = contactsRepository.getContact(senderAccountId)

        Timber.d("Processing incoming request ${data.chatRequestId}, existingContact=${existingContact?.username}, pendingRequestId=${existingContact?.pendingChatRequestId}, hasPushToken=${existingContact?.pushToken != null}")

        when {
            // Existing contact with a pending request
            existingContact != null && existingContact.pendingChatRequestId != null -> {
                handleExistingContactWithRequest(existingContact, data)
            }

            // This chat request has already been processed previously, ignore the request
            chatRequestRepository.getById(data.chatRequestId) != null -> {
                IncomingRequestProcessingResult.Ignored.ChatRequestAlreadyProcessed
            }

            // Established contact re-invites (new request id) -> reconnect, don't ignore.
            existingContact != null -> {
                reestablishContact(existingContact, data)
            }

            // No existing contact - create new
            else -> createNewIncomingRequestChat(data)
        }
    }
        .onSuccess { Timber.d("Processing outcome for request ${data.chatRequestId}: $it") }
        .logFailure("Failed to process incoming chat request")

    override suspend fun processIncomingRequestForOutgoingRequest(
        data: IncomingChatRequestData,
        contact: Contact
    ): Result<IncomingRequestProcessingResult.AutoAccepted> {
        return runCatching {
            val existingRequest = chatRequestRepository.getByIdOrThrow(contact.pendingChatRequestId!!)
            autoAcceptMutualRequest(contact, existingRequest, data)
        }
    }

    private suspend fun handleExistingContactWithRequest(
        contact: Contact,
        data: IncomingChatRequestData
    ): IncomingRequestProcessingResult {
        val existingRequest = chatRequestRepository.getById(contact.pendingChatRequestId!!)
            ?: return run {
                Timber.w("Inconsistent db state: contact ${contact.accountId} has chatRequestId but there is no matching ChatRequest found. Fall-backing to create new request")
                createNewIncomingRequestChat(data, contact.username, contact.chatKey)
            }

        return when (existingRequest.direction) {
            // We sent an outgoing request - auto-accept (both parties want to chat)
            ChatRequest.Direction.OUTGOING -> {
                autoAcceptMutualRequest(contact, existingRequest, data)
            }

            // We already have an incoming request from this peer
            ChatRequest.Direction.INCOMING -> {
                updateIfNewer(contact, existingRequest, data)
            }
        }
    }

    private suspend fun reestablishContact(
        contact: Contact,
        data: IncomingChatRequestData
    ): IncomingRequestProcessingResult {
        // Known contact re-invited (left + came back): recreate the request + auto-accept so both reconnect.
        val created = createNewIncomingRequestChat(data, contact.username, contact.chatKey)
        require(created is IncomingRequestProcessingResult.NewContactCreated)

        acceptIncomingRequestInternal(created.contact)

        return IncomingRequestProcessingResult.AutoAccepted(created.contact)
    }

    private suspend fun autoAcceptMutualRequest(
        contact: Contact,
        ourRequest: ChatRequest,
        incomingData: IncomingChatRequestData
    ): IncomingRequestProcessingResult.AutoAccepted {
        // Clear pendingChatRequestId before status — see markOutgoingRequestAccepted.
        contactsRepository.markChatRequestAccepted(contact.accountId, CurrentTimeContext.currentTime())

        chatRequestRepository.updateStatus(ourRequest.id, ChatRequest.Status.ACCEPTED)

        val updatedContact = updateContactFromRequest(contact, incomingData)

        markOurWelcomeMessageAsRead(ourRequest.welcomeMessageId)

        saveChatAcceptedMessage(contact.accountId, ourRequest.id)

        return IncomingRequestProcessingResult.AutoAccepted(updatedContact)
    }

    override suspend fun acceptIncomingRequest(
        contact: Contact,
    ): Result<Unit> = runCatching {
        acceptIncomingRequestInternal(contact)
    }

    private suspend fun acceptIncomingRequestInternal(contact: Contact) {
        val chatRequestId = contact.pendingChatRequestIdOrThrow()

        contactsRepository.markChatRequestAccepted(contact.accountId, CurrentTimeContext.currentTime())

        chatRequestRepository.updateStatus(chatRequestId, ChatRequest.Status.ACCEPTED)

        saveChatAcceptedMessage(contact.accountId, chatRequestId)
    }

    override suspend fun declineIncomingRequest(contact: Contact): Result<Unit> {
        return runCatching {
            val chatRequestId = contact.pendingChatRequestIdOrThrow()
            chatRequestRepository.updateStatus(chatRequestId, ChatRequest.Status.DECLINED)
        }
    }

    override suspend fun markOutgoingRequestAccepted(
        contact: Contact,
        ourRequest: ChatRequest
    ): Result<Unit> = runCatching {
        // Order matters: clear pendingChatRequestId first, then status. Otherwise the
        // session manager briefly sees "pending request with non-PENDING status" and tears
        // the session down mid-handshake → "Wait for peer to accept" sticks forever.
        // TODO: replace with a single Room transaction across both DAOs.
        contactsRepository.markChatRequestAccepted(contact.accountId, CurrentTimeContext.currentTime())
        chatRequestRepository.updateStatus(ourRequest.id, ChatRequest.Status.ACCEPTED)
    }.also {
        // Welcome row lives on the receiver, not here — swallow the miss.
        runCatching { markOurWelcomeMessageAsRead(ourRequest.id) }
            .onFailure { Timber.w(it, "markOurWelcomeMessageAsRead failed (non-critical)") }
    }

    private suspend fun updateIfNewer(
        contact: Contact,
        existingRequest: ChatRequest,
        incomingData: IncomingChatRequestData
    ): IncomingRequestProcessingResult {
        val newRequest = createNewChatRequest(incomingData)

        return if (newRequest.timestamp > existingRequest.timestamp) {
            // We do explicit update in case messageId (and thus request id) has changed
            chatRequestRepository.delete(existingRequest.id)
            chatRequestRepository.save(newRequest)

            // Update contact link
            contactsRepository.updateChatRequestId(contact.accountId, newRequest.id)

            // Update contact with new push token if provided
            updateContactFromRequest(contact, incomingData)

            // Save welcome message if provided
            saveWelcomeMessage(senderAccountId = contact.accountId, incomingData)

            IncomingRequestProcessingResult.RequestUpdated(newRequest)
        } else {
            // Existing request is same or newer - ignore
            IncomingRequestProcessingResult.Ignored.ChatRequestAlreadyProcessed
        }
    }

    private fun createNewChatRequest(incomingData: IncomingChatRequestData): ChatRequest {
        return ChatRequest(
            welcomeMessageId = incomingData.chatRequestId,
            timestamp = incomingData.request.message.timestamp.toLong(),
            direction = ChatRequest.Direction.INCOMING,
            status = ChatRequest.Status.PENDING
        )
    }

    private val IncomingChatRequestData.chatRequestId: ChatRequestId
        get() = request.message.messageId

    private suspend fun createNewIncomingRequestChat(
        data: IncomingChatRequestData
    ): IncomingRequestProcessingResult {
        val peerAccountId = data.request.peerIdentityAccountId()
        val consumerInfo = resourcesRepository
            .requireConsumerInfo(chainRegistry.getChain(knownChains.assetHub), peerAccountId)
            .getOrThrow()

        return createNewIncomingRequestChat(data, consumerInfo.username, (consumerInfo.identifierKey.x25519OrNull() ?: error("Peer account uses an unsupported chat encryption key type")))
    }

    private suspend fun createNewIncomingRequestChat(
        data: IncomingChatRequestData,
        peerUsername: String?,
        peerChatKey: X25519PublicKey,
    ): IncomingRequestProcessingResult {
        val peerAccountId = data.request.peerIdentityAccountId()

        val request = ChatRequest(
            welcomeMessageId = data.request.message.messageId,
            timestamp = data.request.message.timestamp.toLong(),
            direction = ChatRequest.Direction.INCOMING,
            status = ChatRequest.Status.PENDING
        )
        chatRequestRepository.save(request)

        val content = extractContent(data.request.message)

        // TODO: We have to set Contact.pendingDevicesFanOut as false here and set it as true after accepting chat request to run fan out
        val contact = Contact(
            accountId = peerAccountId,
            username = peerUsername,
            chatKey = peerChatKey,
            ourMetaAccountId = data.ourMetaAccountId,
            avatarUrl = null,
            sharedSecretDerivationDomain = data.derivationDomain,
            pendingChatRequestId = request.id,
            pushToken = content.pushToken?.token?.toDataByteArray(),
            operatingSystem = content.pushToken?.platform.toOperatingSystem(),
            addedAt = CurrentTimeContext.currentTime(),
        )
        Timber.d("createNewIncomingRequestChat: contact=$peerUsername, hasPushToken=${contact.pushToken != null}, os=${contact.operatingSystem}")
        contactsRepository.saveContact(contact)
        chatRoomRepository.createRoomIfNotExists(ChatId.fromContact(peerAccountId))

        savePeerDevice(peerAccountId, peerStatementAccountId = data.request.proof.signerAccountId(), content.peerDeviceEncPubKey)

        // Save welcome message if provided
        saveWelcomeMessage(peerAccountId, data)

        return IncomingRequestProcessingResult.NewContactCreated(contact, request)
    }

    private suspend fun updateContactFromRequest(
        contact: Contact,
        data: IncomingChatRequestData
    ): Contact {
        val content = extractContent(data.request.message)

        savePeerDevice(contact.accountId, peerStatementAccountId = data.request.proof.signerAccountId(), content.peerDeviceEncPubKey)

        if (content.pushToken != null) {
            val pushToken = content.pushToken.token.toDataByteArray()
            val operatingSystem = content.pushToken.platform.toOperatingSystem()
            contactsRepository.updateContactPushToken(contact.accountId, pushToken, operatingSystem)
            return contact.copy(pushToken = pushToken, operatingSystem = operatingSystem)
        }

        return contact
    }

    private suspend fun saveWelcomeMessage(
        senderAccountId: AccountId,
        data: IncomingChatRequestData
    ) {
        val content = extractContent(data.request.message)
        val welcomeMessage = content.welcomeMessage?.toWelcomeContent() ?: return

        val chatMessage = ChatMessage(
            id = data.request.message.messageId,
            chatId = ChatId.fromContact(senderAccountId),
            timestamp = data.request.message.timestamp.toLong(),
            content = ChatMessage.Content.ChatRequest(welcomeMessage),
            origin = ChatMessageOrigin.Contact(senderAccountId),
            status = ChatMessage.Status.NEW
        )
        chatEngine.get().saveMessage(chatMessage)
    }

    private suspend fun markOurWelcomeMessageAsRead(welcomeMessageId: ChatMessageId) {
        chatEngine.get().markMessageAsRead(welcomeMessageId)
    }

    private suspend fun saveChatAcceptedMessage(
        peerAccountId: AccountId,
        requestId: String
    ) {
        val ourPAppDevice = buildPAppDevice()

        // We use a fresh message id here: this will duplicate "Chat accepted" upon reinstall and it is expected for now
        // NB: Using deterministic id is not trivial since it would count as "already present" on recipient side
        // and wont trigger save message processors
        val chatMessage = ChatMessage.new(
            chatId = ChatId.fromContact(peerAccountId),
            content = ChatMessage.Content.DeviceChatAccepted(requestId, ourPAppDevice),
            origin = ChatMessageOrigin.User,
            status = ChatMessage.Status.NEW
        )
        chatEngine.get().saveMessage(chatMessage)
    }

    private suspend fun buildPAppDevice(): DeviceInfo {
        val walletAccount = accountRepository.getWalletAccount()
        return DeviceInfo(
            statementAccountId = walletAccount.defaultAccountId(),
            encryptionPublicKey = ourDeviceKeypairProvider.publicKey(),
        )
    }

    private fun extractContent(message: ChatRequestMessage): NormalizedRequestContent {
        return when (val content = message.content) {
            is VersionedRequestContent.V1 -> NormalizedRequestContent(
                pushToken = content.content.pushToken,
                welcomeMessage = content.content.welcomeMessage,
                peerDeviceEncPubKey = null,
            )

            is VersionedRequestContent.V2 -> NormalizedRequestContent(
                pushToken = content.content.pushToken,
                welcomeMessage = content.content.welcomeMessage,
                peerDeviceEncPubKey = content.content.deviceEncPubKey.toDomain().getOrThrow(),
            )
        }
    }

    // Routed through a synthetic DeviceAdded chat message so the device-sync pipeline
    // (which reads from messages, not ContactDevicesRepository) picks it up; the local
    // device-roster write is then handled by DeviceLifecycleMessageProcessor downstream.
    private suspend fun savePeerDevice(
        contactAccountId: AccountId,
        peerStatementAccountId: AccountId,
        peerDeviceEncPubKey: X25519PublicKey?,
    ) {
        if (peerDeviceEncPubKey == null) return

        val message = ChatMessage.new(
            chatId = ChatId.fromContact(contactAccountId),
            content = ChatMessage.Content.DeviceAdded(
                statementAccountId = peerStatementAccountId,
                encryptionPublicKey = peerDeviceEncPubKey,
            ),
            origin = ChatMessageOrigin.Contact(contactAccountId),
            status = ChatMessage.Status.NEW,
        )
        chatEngine.get().saveMessage(message)
    }

    private data class NormalizedRequestContent(
        val pushToken: TokenContent?,
        val welcomeMessage: RichTextContent?,
        val peerDeviceEncPubKey: X25519PublicKey?,
    )

    private fun RichTextContent.toWelcomeContent(): ChatMessage.Content.RichText {
        return ChatMessage.Content.RichText(
            text = text,
            attachments = emptyList()
        )
    }

    private fun TokenPlatform?.toOperatingSystem(): OperatingSystem {
        return when (this) {
            null -> OperatingSystem.UNKNOWN

            TokenPlatform.ANDROID -> OperatingSystem.ANDROID

            TokenPlatform.IOS,
            TokenPlatform.IOS_VOIP -> OperatingSystem.IOS
        }
    }
}
