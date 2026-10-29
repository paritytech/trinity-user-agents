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
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushId
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSession
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSessionManager
import io.paritytech.polkadotapp.feature_chats_api.domain.isMultiDeviceChatSupported
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.withSessionsEnabled
import io.paritytech.polkadotapp.feature_chats_impl.data.model.isDeviceChatAccepted
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPayloadEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatAcceptanceRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.compaction.ChatMessageCompactor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ContactWithDevicePresence
import io.paritytech.polkadotapp.feature_chats_impl.domain.notifications.ChatPushNotificationsSender
import io.paritytech.polkadotapp.feature_chats_impl.domain.notifications.PushSubscriptionSynchronizer
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer.ContactChatSigners
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SyncContactUsernameUseCase
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatPushTokenUtils
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSessionCreator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.IncomingRequestResponsePolicy
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
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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

// The contact and the session type come from one snapshot of the contact and its devices, so an accepted chat is
// never paired with the pairwise session its acceptance replaces.
private class RunningSession(
    val contact: Contact,
    val sessionType: SessionType,
    val session: RealContactChatSession,
) : Identifiable {
    override val identifier: String = "${contact.identifier}:$sessionType"

    fun withContact(contact: Contact) = RunningSession(contact, sessionType, session)
}

// Acceptances are never acknowledged: the response would be the only statement we sign on the identity route.
private val ACCEPTANCES_UNACKNOWLEDGED = IncomingRequestResponsePolicy { messages ->
    messages.any { message -> !message.isDeviceChatAccepted() }
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
    private val refCounter: ContactChatSessionRefCounter,
    private val pushSubscriptionSynchronizer: PushSubscriptionSynchronizer,
    private val syncContactUsernameUseCase: SyncContactUsernameUseCase,
    private val chatMessageCompactor: ChatMessageCompactor,
    private val incomingChatMessageProcessor: IncomingChatMessageProcessor,
    private val notificationPayloadEncoder: ChatNotificationPayloadEncoder,
    private val proverFactory: StatementStoreMessageProver.Factory,
    private val chatSigners: ContactChatSigners,
    private val chatAcceptanceRepository: ChatAcceptanceRepository,
    dispatchers: CoroutineDispatchers
) : ContactChatSessionManager, ContactChatSessionRestarter, CoroutineScope, ChatSessionCallbacks {
    companion object {
        private val MAX_STATEMENT_SIZE = 100.kilobytes
    }

    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val sessionsFlow = MutableStateFlow<Map<AccountId, RunningSession>>(emptyMap())

    private val sessionsMutex = Mutex()

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
        return sessionsFlow.value[accountId]?.session
    }

    override fun getAllSessions(): List<ContactChatSession> {
        return sessionsFlow.value.values.map { it.session }
    }

    override suspend fun awaitSession(accountId: AccountId): ContactChatSession {
        return sessionsFlow
            .mapNotNull { it[accountId]?.session }
            .first()
    }

    fun startSubscriptions() {
        registerSubscriptionOnTokenChanges()
        subscribeToEnabledContacts()
        subscribeToEstablishedChats()
        subscribeToTokenUpdates()
    }

    override suspend fun restartSession(accountId: AccountId) {
        sessionsMutex.withLock {
            val running = sessionsFlow.value[accountId] ?: return@withLock

            createSession(running.contact, running.sessionType).onSuccess { restarted ->
                running.session.dispose()
                sessionsFlow.update { it + (accountId to RunningSession(running.contact, running.sessionType, restarted)) }
            }
        }
        syncPushRules()
    }

    override suspend fun onContactSeenOnDeviceRoute(contactAccountId: AccountId) {
        chatAcceptanceRepository.remove(contactAccountId)
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
            contactsRepository.subscribeContactsWithDevicePresence(),
        ) { enabledIds, walletAccount, contacts ->
            val byAccountId = contacts.associateBy { it.contact.accountId }
            enabledIds.mapNotNull { accountId -> byAccountId[accountId] }
                .map { ContactWithSessionType(it.contact, determineSessionType(walletAccount, it)) }
        }
            .diffed()
            .onEach { diff ->
                sessionsMutex.withLock { applyContactsDiff(diff) }
                syncPushRules()
            }
            .launchIn(this)
    }

    private fun determineSessionType(walletMetaAccount: MetaAccount, contact: ContactWithDevicePresence): SessionType {
        val isMultiDeviceChatSupported = contact.contact.isMultiDeviceChatSupported(walletMetaAccount)
        return if (contact.hasDevices && isMultiDeviceChatSupported) {
            SessionType.MultiDevice
        } else {
            SessionType.Pairwise
        }
    }

    private suspend fun syncPushRules() {
        val rules = sessionsFlow.value.values.flatMap { it.session.pushRules }
        pushSubscriptionSynchronizer.syncRules(rules)
    }

    // An updated contact keeps its running session; only a new contact or session type gets a new one.
    private suspend fun applyContactsDiff(diff: CollectionDiffer.Diff<ContactWithSessionType>) {
        val added = buildList {
            diff.added.forEach { new ->
                createSession(new.contact, new.sessionType)
                    .onSuccess { session -> add(RunningSession(new.contact, new.sessionType, session)) }
            }
        }
        val removedIds = diff.removed.map { it.contact.accountId }

        val replacedIds = added.map { it.contact.accountId } + removedIds
        replacedIds.forEach { sessionsFlow.value[it]?.session?.dispose() }

        sessionsFlow.update { current ->
            val updated = diff.updated.mapNotNull { new -> current[new.contact.accountId]?.withContact(new.contact) }
            (current - replacedIds.toSet()) + (updated + added).associateBy { it.contact.accountId }
        }
    }

    private suspend fun createSession(contact: Contact, sessionType: SessionType) = runCatching {
        Timber.d("createSession: contact=${contact.username}, sessionType=$sessionType, hasPushToken=${contact.pushToken != null}, hasLastShared=${contact.lastSharedPushToken != null}")

        val sessionCreatorWithAccount = sessionCreatorsCache.getOrCompute(contact.ourMetaAccountId)
        val chatSessionCreator = createChatSessionCreator(contact)

        val scope = childScope(supervised = true)

        val communicationSessions = buildCommunicationSessions(
            contact = contact,
            sessionType = sessionType,
            sessionCreatorWithAccount = sessionCreatorWithAccount,
            chatSessionCreator = chatSessionCreator,
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

        syncIncomingPushId(chatSession, contact)

        chatSession
    }.logFailure("ContactChatSessionManager: failed to create session for contact ${contact.username}")

    private fun createChatSessionCreator(contact: Contact): CommunicationSessionCreator {
        val chatProver = proverFactory.createLazyKeyPairProver { chatSigners.keypairFor(contact) }
        return sessionCreatorFactory.create(prover = chatProver)
    }

    // Pending chats sign nothing until accepted, so an unanswered request never claims a chat slot.
    private fun subscribeToEstablishedChats() {
        sessionsFlow
            .map { sessions -> sessions.values.filter { it.contact.establishedAt != null } }
            .diffed()
            .onEach { diff -> diff.added.forEach { onChatEstablished(it) } }
            .launchIn(this)
    }

    private suspend fun onChatEstablished(chat: RunningSession) {
        chatSigners.warmUp(chat.contact)
        runCancellableCatching { shareOwnPushTokenIfChanged(chat.session, chat.contact) }
            .logFailure("ContactChatSessionManager: failed to share push token with ${chat.contact.username}")
    }

    // TODO: should be removed when fully migrated to push-notifications v2
    private suspend fun shareOwnPushTokenIfChanged(chatSession: RealContactChatSession, contact: Contact) {
        val ownPushToken = pushNotificationsHelper.getCurrentToken() ?: return
        if (ownPushToken == contact.lastSharedPushToken) return

        Timber.d("shareOwnPushTokenIfChanged: sending own token to ${contact.username}")
        chatSession.sendToken(ownPushToken)
        contactsRepository.updateLastSharedPushTokenFor(listOf(contact.accountId), ownPushToken)
    }

    private suspend fun syncIncomingPushId(chatSession: RealContactChatSession, contact: Contact) {
        if (chatSession.incomingPushId == contact.pushId) return

        contactsRepository.updatePushId(contact.accountId, chatSession.incomingPushId)
    }

    // TODO: should be removed when fully migrated to push-notifications v2
    private suspend fun invalidateOwnPushToken(newToken: String) {
        val contactsToUpdate = contactsRepository.getContacts()
            .filter { it.establishedAt != null && it.lastSharedPushToken != newToken }
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
        chatSessionCreator: CommunicationSessionCreator,
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

                val mainSession = chatSessionCreator.createMultiDeviceSession(
                    scope = scope,
                    localAccount = localAccount,
                    remoteAccount = remoteAccount,
                    perDeviceEncryption = perDeviceEncryption,
                    identityChatDomain = contact.sharedSecretDerivationDomain,
                    maxStatementSize = MAX_STATEMENT_SIZE,
                    compactor = chatMessageCompactor,
                )

                // Never signs an acceptance: ours go out as one-shot statements and incoming ones are not acknowledged.
                val identitySession = sessionCreatorWithAccount.creator.createSession(
                    scope = scope,
                    localAccount = localAccount,
                    remoteAccount = remoteAccount,
                    encryption = identityEncryption,
                    maxStatementSize = MAX_STATEMENT_SIZE,
                    compactor = chatMessageCompactor,
                    responsePolicy = ACCEPTANCES_UNACKNOWLEDGED,
                )

                CommunicationSessions.MultiDevice(main = mainSession, identity = identitySession)
            }

            SessionType.Pairwise -> {
                val session = chatSessionCreator.createSession(
                    scope = scope,
                    localAccount = localAccount,
                    remoteAccount = remoteAccount,
                    encryption = identityEncryption,
                    maxStatementSize = MAX_STATEMENT_SIZE,
                    compactor = chatMessageCompactor,
                    responsePolicy = ACCEPTANCES_UNACKNOWLEDGED,
                )
                CommunicationSessions.Pairwise(session)
            }
        }
    }
}
