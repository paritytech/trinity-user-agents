package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.memory.MapCache
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CollectionDiffer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.Identifiable
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushId
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSession
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSessionManager
import io.paritytech.polkadotapp.feature_chats_api.domain.isMultiDeviceChatSupported
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.withSessionsEnabled
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPayloadEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.compaction.ChatMessageCompactor
import io.paritytech.polkadotapp.feature_chats_impl.domain.notifications.ChatPushNotificationsSender
import io.paritytech.polkadotapp.feature_chats_impl.domain.notifications.PushSubscriptionSynchronizer
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SyncContactUsernameUseCase
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatPushTokenUtils
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSessionCreator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

private data class SessionCreatorWithAccount(
    val creator: CommunicationSessionCreator,
    val localSessionAccount: SessionAccount.Local
)

private enum class SessionType { Pairwise, MultiDevice }

private data class ContactWithSessionType(
    val contact: Contact,
    val sessionType: SessionType,
) : Identifiable {
    override val identifier: String = "${contact.identifier}:$sessionType"
}

@Singleton
internal class RealContactChatSessionManager @Inject constructor(
    private val sessionCreatorFactory: CommunicationSessionCreator.Factory,
    private val chatMessageRepository: ChatMessageRepository,
    private val contactsRepository: ContactsRepository,
    private val accountRepository: AccountRepository,
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val pushNotificationsHelper: PushNotificationsHelper,
    private val chatPushNotificationsSender: ChatPushNotificationsSender,
    private val encryptionFactory: CommunicationEncryption.Factory,
    private val chatEngine: ChatEngine,
    private val contactDevicesRepository: ContactDevicesRepository,
    private val refCounter: ContactChatSessionRefCounter,
    private val pushSubscriptionSynchronizer: PushSubscriptionSynchronizer,
    private val syncContactUsernameUseCase: SyncContactUsernameUseCase,
    private val chatMessageCompactor: ChatMessageCompactor,
    private val incomingChatMessageProcessor: IncomingChatMessageProcessor,
    private val notificationPayloadEncoder: ChatNotificationPayloadEncoder,
    dispatchers: CoroutineDispatchers
) : ContactChatSessionManager, CoroutineScope, ChatSessionCallbacks {
    companion object {
        private val MAX_STATEMENT_SIZE = 100.kilobytes
    }

    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val sessionsFlow = MutableStateFlow<Map<AccountId, RealContactChatSession>>(emptyMap())

    private val sessionCreatorsCache: MapCache<Long, SessionCreatorWithAccount> = MapCache(this) { ourMetaAccountId ->
        val account = accountRepository.getAccountById(ourMetaAccountId)!!

        val localAccountId = account.accountIdIn(chainRegistry.getChain(knownChains.people))
        val localSessionAccount = SessionAccount.Local(localAccountId, null)

        SessionCreatorWithAccount(
            creator = sessionCreatorFactory.create(account = account),
            localSessionAccount = localSessionAccount
        )
    }

    override fun getSession(accountId: AccountId): ContactChatSession? {
        return sessionsFlow.value[accountId]
    }

    override fun getAllSessions(): List<ContactChatSession> {
        return sessionsFlow.value.values.toList()
    }

    override suspend fun awaitSession(accountId: AccountId): ContactChatSession {
        return sessionsFlow
            .mapNotNull { it[accountId] }
            .first()
    }

    fun startSubscriptions() {
        registerSubscriptionOnTokenChanges()
        subscribeToEnabledContacts()
        subscribeToTokenUpdates()
    }

    override suspend fun onShouldNotifyNewMessageSent(
        messageId: ChatMessageId,
        accountId: AccountId,
        pushId: ChatPushId,
        encryptedMessage: ByteArray,
        isVoIP: Boolean
    ) {
        contactsRepository.getContact(accountId)?.let { contact ->
            val useVoipToken = isVoIP && contact.voipPushToken != null
            val token = if (useVoipToken) contact.voipPushToken else contact.pushToken

            if (token != null) {
                val platformToken = ChatPushTokenUtils.getPlatformToken(token, contact.operatingSystem)
                chatPushNotificationsSender.sendPushNotificationOnce(
                    messageId = messageId,
                    platformToken = platformToken,
                    operatingSystem = contact.operatingSystem,
                    pushId = pushId.value,
                    encryptedMessage = encryptedMessage,
                    isVoIP = useVoipToken
                )
            } else {
                Timber.i("Contact ${contact.username} has no pushToken, cannot send notification")
            }
        }
    }

    // TODO: should be removed when fully migrated to push-notifications v2
    private fun subscribeToTokenUpdates() {
        pushNotificationsHelper
            .subscribeTokenChanges()
            .drop(1) // no need to invalidate on the initial value since it's invalidated on session creation
            .onEach { newToken ->
                Timber.d("subscribeToTokenUpdates: token=${if (newToken != null) "present" else "null"}")
                if (newToken != null) {
                    invalidateOwnPushToken(newToken)
                }
            }
            .launchIn(this)
    }

    private fun subscribeToEnabledContacts() {
        combine(
            refCounter.enabledIds,
            accountRepository.walletAccountFlow(),
            contactsRepository.subscribeContactsWithChatRequests(),
            contactDevicesRepository.subscribeAllDevices(),
        ) { enabledIds, walletAccount, contacts, devicesByContact ->
            val byAccountId = contacts
                .associateBy { it.contact.accountId }
            enabledIds.mapNotNull { accountId -> byAccountId[accountId]?.contact }
                .map { contact ->
                    ContactWithSessionType(
                        contact = contact,
                        sessionType = determineSessionType(walletAccount, contact, devicesByContact),
                    )
                }
        }
            .diffed()
            .onEach { diff ->
                applyContactsDiff(diff)
                syncPushRules()
            }
            .launchIn(this)
    }

    private fun determineSessionType(
        walletMetaAccount: MetaAccount,
        contact: Contact,
        devicesByContact: Map<ContactAccountId, List<ContactDevice>>,
    ): SessionType {
        val isMultiDeviceChatSupported = contact.isMultiDeviceChatSupported(walletMetaAccount)
        val devices = devicesByContact[contact.accountId].orEmpty()
        return if (devices.isNotEmpty() && isMultiDeviceChatSupported) {
            SessionType.MultiDevice
        } else {
            SessionType.Pairwise
        }
    }

    private suspend fun syncPushRules() {
        val rules = sessionsFlow.value.values.flatMap(RealContactChatSession::pushRules)
        pushSubscriptionSynchronizer.syncRules(rules)
    }

    private suspend fun applyContactsDiff(diff: CollectionDiffer.Diff<ContactWithSessionType>) {
        val addedSessions = buildList {
            diff.added.forEach {
                createSession(it.contact, it.sessionType)
                    .onSuccess { session ->
                        add(it.contact.accountId to session)
                    }
            }
        }
        val removedIds = diff.removed.map { it.contact.accountId }

        val replacedIds = addedSessions.map { it.first } + removedIds
        replacedIds.forEach { sessionsFlow.value[it]?.dispose() }

        sessionsFlow.update { current ->
            (current - replacedIds.toSet()) + addedSessions.toMap()
        }
    }

    private suspend fun createSession(contact: Contact, sessionType: SessionType) = runCatching {
        Timber.d("createSession: contact=${contact.username}, sessionType=$sessionType, hasPushToken=${contact.pushToken != null}, hasLastShared=${contact.lastSharedPushToken != null}")

        val sessionCreatorWithAccount = sessionCreatorsCache.getOrCompute(contact.ourMetaAccountId)

        val scope = childScope(supervised = true)

        val communicationSessions = buildCommunicationSessions(
            contact = contact,
            sessionType = sessionType,
            sessionCreatorWithAccount = sessionCreatorWithAccount,
            scope = scope,
        )

        val chatSession = RealContactChatSession(
            scope = scope,
            contact = contact,
            communicationSessions = communicationSessions,
            chatMessageRepository = chatMessageRepository,
            callbacks = this,
            chatEngine = chatEngine,
            syncContactUsernameUseCase = syncContactUsernameUseCase,
            incomingChatMessageProcessor = incomingChatMessageProcessor,
            notificationPayloadEncoder = notificationPayloadEncoder,
        )

        invalidateContact(chatSession, contact)

        chatSession
    }.logFailure("ContactChatSessionManager: failed to create session for contact ${contact.username}")

    // TODO: should be removed when fully migrated to push-notifications v2
    private suspend fun invalidateContact(
        chatSession: RealContactChatSession,
        currentContact: Contact
    ) {
        val ownPushToken = pushNotificationsHelper.getCurrentToken()

        Timber.d("invalidateContact: contact=${currentContact.username}, ownToken=${if (ownPushToken != null) "present" else "null"}, lastShared=${currentContact.lastSharedPushToken != null}")

        if (ownPushToken != null && ownPushToken != currentContact.lastSharedPushToken) {
            Timber.d("invalidateContact: sending own token to ${currentContact.username}")
            chatSession.sendToken(ownPushToken)
        }

        if (chatSession.incomingPushId != currentContact.pushId) {
            contactsRepository.updatePushId(currentContact.accountId, chatSession.incomingPushId)
        }
        if (ownPushToken != null && ownPushToken != currentContact.lastSharedPushToken) {
            contactsRepository.updateLastSharedPushTokenFor(listOf(currentContact.accountId), ownPushToken)
        }
    }

    // TODO: should be removed when fully migrated to push-notifications v2
    private suspend fun invalidateOwnPushToken(newToken: String) {
        val contactsToUpdate = contactsRepository.getContacts()
            .filter { it.lastSharedPushToken != newToken }
        if (contactsToUpdate.isEmpty()) return

        val accountIds = contactsToUpdate.mapToSet { it.accountId }

        refCounter.withSessionsEnabled(accountIds, "TokenSync") {
            val sentTo = mutableListOf<AccountId>()
            contactsToUpdate.forEach { contact ->
                awaitSession(contact.accountId).sendToken(newToken)
                sentTo.add(contact.accountId)
            }
            contactsRepository.updateLastSharedPushTokenFor(sentTo, newToken)
        }
    }

    private fun registerSubscriptionOnTokenChanges() {
        pushNotificationsHelper.subscribeTokenChanges()
            .filterNotNull()
            .distinctUntilChanged()
            .onEach { token -> pushSubscriptionSynchronizer.registerSubscription(token) }
            .launchIn(this)
    }

    private fun Contact.toSessionAccount(): SessionAccount.Remote {
        return SessionAccount.Remote(
            accountId = accountId,
            pin = pin,
            publicKey = chatKey
        )
    }

    private suspend fun buildCommunicationSessions(
        contact: Contact,
        sessionType: SessionType,
        sessionCreatorWithAccount: SessionCreatorWithAccount,
        scope: CoroutineScope,
    ): CommunicationSessions {
        val localAccount = sessionCreatorWithAccount.localSessionAccount
        val remoteAccount = contact.toSessionAccount()
        val identityEncryption = encryptionFactory.create(
            domain = contact.sharedSecretDerivationDomain,
            peerPublicKey = contact.chatKey,
        )

        return when (sessionType) {
            SessionType.MultiDevice -> {
                val perDeviceEncryption = encryptionFactory.createWithDeviceKeypair(contact.chatKey)

                val mainSession = sessionCreatorWithAccount.creator.createMultiDeviceSession(
                    scope = scope,
                    localAccount = localAccount,
                    remoteAccount = remoteAccount,
                    perDeviceEncryption = perDeviceEncryption,
                    identityChatDomain = contact.sharedSecretDerivationDomain,
                    maxStatementSize = MAX_STATEMENT_SIZE,
                    compactor = chatMessageCompactor,
                )

                val identitySession = sessionCreatorWithAccount.creator.createSession(
                    scope = scope,
                    localAccount = localAccount,
                    remoteAccount = remoteAccount,
                    encryption = identityEncryption,
                    maxStatementSize = MAX_STATEMENT_SIZE,
                    compactor = chatMessageCompactor,
                )

                CommunicationSessions.MultiDevice(main = mainSession, identity = identitySession)
            }

            SessionType.Pairwise -> {
                val session = sessionCreatorWithAccount.creator.createSession(
                    scope = scope,
                    localAccount = localAccount,
                    remoteAccount = remoteAccount,
                    encryption = identityEncryption,
                    maxStatementSize = MAX_STATEMENT_SIZE,
                    compactor = chatMessageCompactor,
                )
                CommunicationSessions.Pairwise(session)
            }
        }
    }
}
