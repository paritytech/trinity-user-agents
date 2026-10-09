package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushId
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSession
import io.paritytech.polkadotapp.feature_chats_api.domain.model.*
import io.paritytech.polkadotapp.feature_chats_impl.data.model.decodeAlwaysDecodableChatMessagePart
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPayloadEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.isDisplayableAsPush
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SyncContactUsernameUseCase
import io.paritytech.polkadotapp.feature_chats_impl.utils.ChatPushTokenUtils
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionEvent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.tools_push_notifications_api.NotifyType
import io.paritytech.polkadotapp.tools_push_notifications_api.PushRule
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import timber.log.Timber
import kotlin.reflect.KClass

private val PUSH_ID_PREFIX = "notification".toByteArray(Charsets.UTF_8)

// Single source of truth for which content goes through the identity session: drives both routing
// (sessionFor) and the per-session SentMessagesNotFound reset, so they can't drift apart.
private val IDENTITY_SESSION_CONTENT_TYPES = listOf<KClass<out ChatMessage.Content>>(
    ChatMessage.Content.DeviceChatAccepted::class
)

class RealContactChatSession(
    scope: CoroutineScope,
    private val contact: Contact,
    private val communicationSessions: CommunicationSessions,
    private val chatMessageRepository: ChatMessageRepository,
    private val callbacks: ChatSessionCallbacks,
    private val chatEngine: ChatEngine,
    private val syncContactUsernameUseCase: SyncContactUsernameUseCase,
    private val incomingChatMessageProcessor: IncomingChatMessageProcessor,
    private val notificationPayloadEncoder: ChatNotificationPayloadEncoder,
) : ContactChatSession, CoroutineScope by scope {
    private val mainCommunicationSession get() = communicationSessions.main

    private val pushNotificationSession get() = communicationSessions.identity

    private val chatId = ChatId.fromContact(mainCommunicationSession.remoteAccount.accountId)

    override val incomingPushId: ChatPushId = pushNotificationSession
        .generateSharedIncomingSessionValue(PUSH_ID_PREFIX)
        .toDataByteArray()

    override val outgoingPushId: ChatPushId = pushNotificationSession
        .generateSharedOutgoingSessionValue(PUSH_ID_PREFIX)
        .toDataByteArray()

    val pushRules: List<PushRule> = listOf(
        PushRule(
            senderPubKey = pushNotificationSession.remoteAccount.accountId,
            topic = pushNotificationSession.incomingSessionId.toDataByteArray(),
            notifyType = NotifyType.ALERT
        )
    )

    init {
        communicationSessions.distinct.forEach(::subscribeSessionEvents)

        subscribeMessagesUpdates()
        subscribeUsernameUpdates()

        launch { tryReparseUnsupportedMessages() }
    }

    private fun subscribeUsernameUpdates() {
        syncContactUsernameUseCase.sync(contact.accountId)
            .logFailure("Failed to sync username for contact ${contact.accountId}")
            .launchIn(this)
    }

    override suspend fun sendToken(token: String) {
        val encoded = ChatPushTokenUtils.createAndroidToken(token)
        chatEngine.sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.Token(encoded, OperatingSystem.ANDROID)
        )
    }

    private fun subscribeMessagesUpdates() {
        chatEngine
            .subscribeOutgoingMessagesByStatus(chatId, ChatMessage.Status.NEW)
            .map { messages -> messages.filter(::shouldSend) }
            .diffed()
            .onEach { diff ->
                for (message in diff.added) {
                    trySendMessage(message)
                }
            }
            .launchIn(this)

        chatEngine
            .subscribeOutgoingMessagesByStatus(chatId, ChatMessage.Status.IS_SENT)
            .map { messages -> messages.filter(::shouldNotify) }
            .diffed()
            .onEach { diff ->
                for (message in diff.added) {
                    tryNotifyNewMessageSent(message)
                }
            }
            .launchIn(this)
    }

    private fun trySendMessage(message: ChatMessage) {
        val session = sessionFor(message)
        message.toEncodedMessage()
            .map { session.sendMessage(it) }
            .logFailure("Failed to send message: ${message.id}")
    }

    private fun sessionFor(message: ChatMessage): CommunicationSession {
        return if (message.content::class in IDENTITY_SESSION_CONTENT_TYPES) {
            communicationSessions.identity
        } else {
            communicationSessions.main
        }
    }

    private fun shouldSend(message: ChatMessage): Boolean {
        return message.content !is ChatMessage.Content.ChatRequest &&
            // compaction message is sent internally in Active state of CommunicationSession state machine
            message.content !is ChatMessage.Content.CompactionCommit
    }

    private fun shouldNotify(message: ChatMessage): Boolean = message.content.isDisplayableAsPush()

    private fun subscribeSessionEvents(roledSession: RoledCommunicationSession) {
        val session = roledSession.session
        session
            .subscribeEvents()
            .onEach { event ->
                when (event) {
                    is CommunicationSessionEvent.MessageIsTooLarge -> {
                        handleMessageTooLarge(event.message)
                    }

                    is CommunicationSessionEvent.NewMessagesReceived -> {
                        handleNewMessagesReceived(session, event.messages)
                    }

                    is CommunicationSessionEvent.ResponseReceived -> {
                        handleResponseReceived(event.respondedMessages)
                    }

                    is CommunicationSessionEvent.MessagesSentSuccessfully -> {
                        handleMessagesSentSuccessfully(event.messages)
                    }

                    is CommunicationSessionEvent.MessagesCompacted -> {
                        handleMessagesCompacted(event.compacted, event.originals)
                    }

                    is CommunicationSessionEvent.SessionFailed -> {
                        // TODO: what should we do here?
                    }

                    is CommunicationSessionEvent.SentMessagesNotFound -> {
                        handleSentMessagesNotFound(roledSession.role)
                    }

                    is CommunicationSessionEvent.MessagesFailedToSend -> {
                        // Ignore. Separate job ensures message are always re-sent until delivered
                    }
                }
            }
            .launchIn(this)
    }

    private suspend fun handleNewMessagesReceived(source: CommunicationSession, messages: List<EncodedMessage>) {
        Timber.d("handleNewMessagesReceived: ${messages.size} messages from ${contact.username}")

        incomingChatMessageProcessor.processRaw(source.remoteAccount.accountId, messages, ChatMessagePlacement.Latest)
    }

    private suspend fun tryReparseUnsupportedMessages() {
        val unsupportedMessages = chatMessageRepository.getUnsupportedMessages(chatId)

        for (unsupportedMessage in unsupportedMessages) {
            val rawContent = (unsupportedMessage.content as? ChatMessage.Content.Unsupported)?.rawContent
                ?: continue

            val parsedMessage = rawContent.toChatMessage(
                authorAccountId = mainCommunicationSession.remoteAccount.accountId,
                contactAccountId = mainCommunicationSession.remoteAccount.accountId,
                messageStatus = unsupportedMessage.status
            ).getOrNull() ?: continue

            // Explicitly using replace here since we actually want to overwrite content of previously non-parcelable message
            chatEngine.saveMessage(parsedMessage, ChatMessageSaveConflictStrategy.REPLACE)
            incomingChatMessageProcessor.processParsed(contact.accountId, parsedMessage)
        }
    }

    private suspend fun handleMessageTooLarge(encodedMessage: EncodedMessage) {
        Timber.e("Message too large: ${encodedMessage.size.bytes}")

        val versionedMessage = encodedMessage.decodeAlwaysDecodableChatMessagePart().getOrNull() ?: return
        chatMessageRepository.updateMessageStatus(versionedMessage.id, ChatMessage.Status.DELIVERY_FAILED)
    }

    private suspend fun handleResponseReceived(
        messages: List<EncodedMessage>
    ) {
        for (encodedMessage in messages) {
            val versionedMessage = encodedMessage.decodeAlwaysDecodableChatMessagePart().getOrNull() ?: continue
            chatMessageRepository.updateMessageStatus(
                messageId = versionedMessage.id,
                status = ChatMessage.Status.IS_READ
            )
            if (encodedMessage.isCompactionCommit()) {
                chatMessageRepository.propagateStatusToCompactedMessages(versionedMessage.id, ChatMessage.Status.IS_READ)
            }
        }
    }

    private suspend fun handleMessagesSentSuccessfully(
        messages: List<EncodedMessage>
    ) {
        for (encodedMessage in messages) {
            val messagePart = encodedMessage.decodeAlwaysDecodableChatMessagePart().getOrNull() ?: continue
            chatMessageRepository.updateMessageStatus(
                messagePart.id,
                ChatMessage.Status.IS_SENT
            )
            if (encodedMessage.isCompactionCommit()) {
                chatMessageRepository.propagateStatusToCompactedMessages(messagePart.id, ChatMessage.Status.IS_SENT)
            }
        }
    }

    private fun EncodedMessage.isCompactionCommit(): Boolean {
        val message = toChatMessage(
            authorAccountId = mainCommunicationSession.localAccount.accountId,
            contactAccountId = mainCommunicationSession.remoteAccount.accountId,
            messageStatus = ChatMessage.Status.NEW
        ).getOrNull()

        return message?.content is ChatMessage.Content.CompactionCommit
    }

    private suspend fun handleMessagesCompacted(compacted: EncodedMessage, originals: List<EncodedMessage>) {
        val commitMessage = compacted.toChatMessage(
            authorAccountId = mainCommunicationSession.localAccount.accountId,
            contactAccountId = mainCommunicationSession.remoteAccount.accountId,
            messageStatus = ChatMessage.Status.NEW
        ).logFailure("Failed to decode compaction commit").getOrNull() ?: return

        val originalIds = originals.mapNotNull { it.decodeAlwaysDecodableChatMessagePart().getOrNull()?.id }

        chatEngine.saveMessage(commitMessage, ChatMessageSaveConflictStrategy.IGNORE)
        chatMessageRepository.commitCompaction(commitMessage.id, originalIds)
    }

    // A session's "not found" must reset only the messages routed to it (see sessionFor / the session's role).
    private suspend fun handleSentMessagesNotFound(role: ContactSessionRole) {
        when (role) {
            ContactSessionRole.Combined -> {
                chatMessageRepository.unwindUnackedCompactions(chatId)
                chatMessageRepository.updateOutgoingMessagesStatusForChat(
                    chatId = chatId,
                    fromStatus = ChatMessage.Status.IS_SENT,
                    toStatus = ChatMessage.Status.NEW,
                )
            }

            ContactSessionRole.Identity ->
                chatMessageRepository.updateOutgoingMessagesStatusForChatWithTypes(
                    chatId = chatId,
                    fromStatus = ChatMessage.Status.IS_SENT,
                    toStatus = ChatMessage.Status.NEW,
                    contentTypes = IDENTITY_SESSION_CONTENT_TYPES,
                )

            ContactSessionRole.MultiDevice -> {
                chatMessageRepository.unwindUnackedCompactions(chatId)
                chatMessageRepository.updateOutgoingMessagesStatusForChatExcludingTypes(
                    chatId = chatId,
                    fromStatus = ChatMessage.Status.IS_SENT,
                    toStatus = ChatMessage.Status.NEW,
                    contentTypes = IDENTITY_SESSION_CONTENT_TYPES,
                )
            }
        }
    }

    private suspend fun tryNotifyNewMessageSent(message: ChatMessage) {
        val isVoIP = message.content is ChatMessage.Content.DataChannelOffer

        notificationPayloadEncoder.encode(message)
            .map { pushNotificationSession.encrypt(it) }
            .map { encrypted ->
                callbacks.onShouldNotifyNewMessageSent(
                    messageId = message.id,
                    accountId = pushNotificationSession.remoteAccount.accountId,
                    pushId = outgoingPushId,
                    encryptedMessage = encrypted,
                    isVoIP = isVoIP
                )
            }.logFailure("Failed to notify that new message was sent")
    }

    fun dispose() {
        cancel()
    }
}
