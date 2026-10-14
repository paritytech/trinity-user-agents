package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.x25519OrNull
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.model.ConsumerInfo
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.AddContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigins
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.TokenPlatform
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Instant

class RealAddContactUseCase @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val outgoingChatRequestService: OutgoingChatRequestService,
    private val chatRequestRepository: ChatRequestRepository,
    private val chatRoomRepository: ChatRoomRepository,
    private val pushNotificationsHelper: PushNotificationsHelper,
    private val chatEngine: ChatEngine,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val resourcesRepository: ResourcesRepository,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
) : AddContactUseCase {
    override suspend fun addContactWithChatRequest(
        contactAccountId: AccountId,
        username: Username?,
        avatar: String?,
        chatKey: ByteArray,
        sharedSecretDerivationDomain: SharedSecretDerivationDomain,
        ourMetaAccountId: Long,
        origin: ContactOrigin,
        welcomeMessage: ChatMessage.Content.RichText?,
    ): Result<Unit> = withContext(coroutineDispatchers.io) {
        val token = pushNotificationsHelper.getCurrentToken()
        val tokenContent = createTokenContent(token)

        val contact = Contact(
            accountId = contactAccountId,
            username = username?.getDisplayUsername(),
            chatKey = chatKey.requireX25519PublicKey(),
            sharedSecretDerivationDomain = sharedSecretDerivationDomain,
            ourMetaAccountId = ourMetaAccountId,
            avatarUrl = avatar,
            origin = origin,
            lastSharedPushToken = token,
            addedAt = CurrentTimeContext.currentTime(),
        )

        Timber.d("Sending new chat request to contact ${contact.accountId}, welcome message: $welcomeMessage")

        outgoingChatRequestService.sendChatRequest(contact, tokenContent, welcomeMessage)
            .flatMap { chatRequest ->
                createPendingContactChat(contact, chatRequest, welcomeMessage)
            }
    }

    override suspend fun addAlreadyEstablishedContactsById(accountIds: List<AccountId>): Result<Unit> {
        if (accountIds.isEmpty()) return Result.success(Unit)

        val chain = chainRegistry.getChain(knownChains.assetHub)
        return resourcesRepository.resolveConsumers(chain.id, accountIds).flatMap { consumerInfoByAccount ->
            runCatching {
                val walletAccount = accountRepository.getWalletAccount()
                val now = CurrentTimeContext.currentTime()

                accountIds.forEach { accountId ->
                    val consumerInfo = consumerInfoByAccount[accountId] ?: run {
                        Timber.w("addAlreadyEstablishedContactsById: no ConsumerInfo for 0x%s", accountId.value.toHexString())
                        return@forEach
                    }
                    runCatching {
                        val contact = consumerInfo.toAlreadyEstablishedContact(walletAccount.id, now)
                        contactsRepository.saveContact(contact)
                        chatRoomRepository.createRoomIfNotExists(ChatId.fromContact(contact.accountId))
                    }.onFailure {
                        Timber.w(it, "addAlreadyEstablishedContactsById: save failed for 0x${accountId.value.toHexString()}")
                    }
                }
            }
        }
    }

    override suspend fun getContactsAddedAfter(after: Instant): List<Contact> {
        return contactsRepository.getAddedAfter(after)
    }

    override suspend fun getEstablishedContactsAddedAfter(after: Instant): List<Contact> {
        return contactsRepository.getEstablishedAfter(after)
    }

    override fun subscribeContactAccountIds(): Flow<Set<AccountId>> {
        return contactsRepository.subscribeContacts()
            .map { contacts -> contacts.mapToSet { it.accountId } }
    }

    override fun observeContactsChanged(): Flow<Unit> = contactsRepository.observeContactsChanged()

    private fun ConsumerInfo.toAlreadyEstablishedContact(ourMetaAccountId: Long, addedAt: Instant): Contact {
        return Contact(
            accountId = accountId,
            username = username,
            chatKey = identifierKey.x25519OrNull() ?: error("Peer account uses an unsupported chat encryption key type"),
            ourMetaAccountId = ourMetaAccountId,
            avatarUrl = null,
            sharedSecretDerivationDomain = SharedSecretDerivationDomain.CHAT,
            origin = ContactOrigins.CONTACT_CHAT,
            addedAt = addedAt,
            establishedAt = addedAt,
        )
    }

    private fun createTokenContent(token: String?): TokenContent? {
        return token?.let {
            TokenContent(
                token = token.toByteArray(Charsets.UTF_8),
                platform = TokenPlatform.ANDROID
            )
        }
    }

    private suspend fun createPendingContactChat(
        contact: Contact,
        chatRequest: ChatRequest,
        welcomeMessage: ChatMessage.Content.RichText?,
    ): Result<Unit> {
        return runCatching {
            chatRequestRepository.save(chatRequest)

            val contactWithRequestId = contact.copy(pendingChatRequestId = chatRequest.id)
            contactsRepository.saveContact(contactWithRequestId)
            chatRoomRepository.createRoomIfNotExists(ChatId.fromContact(contact.accountId))

            if (welcomeMessage != null) {
                saveWelcomeMessage(contact, chatRequest, welcomeMessage)
            }
        }
    }

    private suspend fun saveWelcomeMessage(
        contact: Contact,
        chatRequest: ChatRequest,
        welcomeMessage: ChatMessage.Content.RichText?
    ) {
        val chatMessage = ChatMessage(
            id = chatRequest.id,
            chatId = ChatId.fromContact(contact.accountId),
            timestamp = chatRequest.timestamp,
            content = ChatMessage.Content.ChatRequest(welcomeMessage),
            origin = ChatMessageOrigin.User,
            status = ChatMessage.Status.IS_SENT
        )

        chatEngine.saveMessage(chatMessage)
    }
}
