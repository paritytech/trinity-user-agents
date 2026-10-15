package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import android.net.Uri
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getAccountByIdOrThrow
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomActivityRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSessionManager
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.AddContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.RemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isOutgoing
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isUser
import io.paritytech.polkadotapp.feature_chats_api.domain.model.onContact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.userDeclinedIncomingRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import io.paritytech.polkadotapp.feature_chats_impl.data.AttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload.FileUploadStarter
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPublisher
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.ProductAttachmentEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getContactResult
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.AttachmentFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.FileTooLargeException
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatActiveTrackerInternal
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.error.ChatRequestError
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPageOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContext
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomSourceContextFactory
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.InitiateCallResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageEditHistoryItem
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.DmChatMessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.productQueryMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductRoomUserActions
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.flow.flowOf as flowOfValue

private val MAX_ATTACHMENT_SIZE = 128.megabytes

class ChatFeedInteractor @Inject internal constructor(
    private val chatMessageRepository: ChatMessageRepository,
    private val chatDraftRepository: ChatDraftRepository,
    private val chatSessionManager: ContactChatSessionManager,
    private val chatNotificationPublisher: ChatNotificationPublisher,
    private val chatActiveTracker: ChatActiveTrackerInternal,
    private val chatEngine: ChatEngine,
    private val contactsRepository: ContactsRepository,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val chatDisplayGenerator: ChatDisplayGenerator,
    private val dmChatMessageOriginDisplayResolver: DmChatMessageOriginDisplayResolver.Factory,
    private val accountRepository: AccountRepository,
    private val addContactUseCase: AddContactUseCase,
    private val removeContactUseCase: RemoveContactUseCase,
    private val incomingChatRequestProcessor: IncomingChatRequestProcessor,
    private val callController: CallController,
    private val callStateTracker: CallStateTracker,
    private val fileUploadRepository: FileUploadRepository,
    private val fileUploadStarter: FileUploadStarter,
    private val attachmentMetaBuilder: AttachmentMetaBuilder,
    private val attachmentFileStorage: AttachmentFileStorage,
    private val hopNodeUrlProvider: HopNodeUrlProvider,
    private val roomPolicyProvider: ChatRoomPolicyProvider,
    private val productRoomUserActions: ProductRoomUserActions,
    private val roomSourceContextFactory: ChatRoomSourceContextFactory,
    private val productAttachmentEncoder: ProductAttachmentEncoder,
    private val activityRepository: ChatRoomActivityRepository,
    private val activityWriter: ChatRoomActivityWriter,
) {
    private val roomSources = ConcurrentHashMap<ChatId, ChatRoomSourceContext>()

    fun initiateCall(chatId: ChatId, callerName: String, withVideo: Boolean): InitiateCallResult {
        val activeCall = callStateTracker.getActiveCall()

        return when {
            activeCall == null -> {
                callController.initiateCall(chatId, callerName, withVideo)
                InitiateCallResult.DONE
            }

            activeCall.chatId == chatId -> {
                callController.openOngoingCallScreen()
                InitiateCallResult.DONE
            }

            else -> InitiateCallResult.BUSY_IN_ANOTHER_CHAT
        }
    }

    fun subscribeUserInputState(request: OpenChatRequest): Flow<ChatUserInputState> {
        return when (request) {
            is OpenChatRequest.ExistingChat -> subscribeUserInputState(request.chatId)

            is OpenChatRequest.StartChatWithContact -> subscribeUserInputStateForContact(request.contactAccountId)
        }
    }

    fun observeChatDisplay(request: OpenChatRequest): Flow<Result<ChatDisplay>> {
        return when (request) {
            is OpenChatRequest.ExistingChat -> chatEngine.observeChatDisplay(request.chatId)
            is OpenChatRequest.StartChatWithContact -> flowOf { Result.success(getChatDisplay(request)) }
        }.inBackground()
    }

    suspend fun getHeaderRendererForChat(chatId: ChatId): CustomChatHeaderRenderer? =
        chatEngine.getHeaderRendererForChat(chatId)

    suspend fun getMessageOriginDisplayResolver(request: OpenChatRequest): MessageOriginDisplayResolver {
        return when (request) {
            is OpenChatRequest.ExistingChat -> chatEngine.originDisplayResolver(request.chatId)

            is OpenChatRequest.StartChatWithContact -> dmChatMessageOriginDisplayResolver.create(
                chatDisplay = getChatDisplay(request),
                ourAccountId = accountRepository.getAccountByIdOrThrow(request.ourMetaAccountId)
                    .defaultAccountId()
            )
        }
    }

    suspend fun addUserReaction(chatId: ChatId, messageId: ChatMessageId, content: ChatMessageReactionContent) {
        val reaction = ChatMessageReaction(
            messageId = messageId,
            content = content,
            origin = ChatMessageOrigin.User,
            timestamp = System.currentTimeMillis()
        )

        chatMessageRepository.addReaction(reaction, chatId)
        chatEngine.sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.Reacted(messageId, content)
        )
    }

    suspend fun removeUserReaction(chatId: ChatId, reaction: ChatMessageReaction) {
        chatMessageRepository.removeMessageReaction(reaction, chatId)
        chatEngine.sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.ReactionRemoved(reaction.messageId, reaction.content)
        )
    }

    /** The window of messages of the room, from the host database or from the product (02-DESIGN §8.3). */
    fun subscribeMessages(chatId: ChatId): Flow<ChatFeedSnapshot> = flow {
        emitAll(roomSource(chatId).source().observeSnapshots())
    }

    suspend fun loadOlderMessages(chatId: ChatId): ChatPageOutcome = roomSource(chatId).source().loadOlder()

    suspend fun loadNewerMessages(chatId: ChatId): ChatPageOutcome = roomSource(chatId).source().loadNewer()

    /** Replaces the window with the newest page of the room. */
    suspend fun showNewestMessages(chatId: ChatId) = roomSource(chatId).source().reload()

    /** Pages until the message is in the window. False when the room has no such message. */
    suspend fun jumpToMessage(chatId: ChatId, messageId: ChatMessageId): Boolean = roomSource(chatId).source().jump(messageId)

    /** Stops the source of the room and closes the room when it is open. Call it when the room screen goes away. */
    fun releaseRoom(chatId: ChatId) {
        roomSources.remove(chatId)?.release()
    }

    suspend fun sendTextMessage(
        chatId: ChatId,
        text: String,
        replyToMessageId: ChatMessageId?
    ): ChatSourceActionOutcome {
        // Only a command that the room declares goes to the product; any other `/text` is a message.
        if (chatEngine.handleCommand(chatId, text)) return ChatSourceActionOutcome.Done
        val content = ChatMessage.Content.Text(text)

        performInProductRoom(chatId, ChatSourceAction.Send(content, replyToMessageId))?.let { return it }

        chatEngine.sendUserMessage(
            chatId = chatId,
            content = content,
            replyToMessageId = replyToMessageId
        )
        return ChatSourceActionOutcome.Done
    }

    /**
     * Adds the reaction of the user, or removes it when the user has it. In a host-store room the user has one
     * reaction a message, so a new emoji replaces the old one.
     */
    suspend fun toggleUserReaction(
        chatId: ChatId,
        messageId: ChatMessageId,
        emoji: String,
        messageReactions: List<ChatMessageReaction>,
    ): ChatSourceActionOutcome {
        performInProductRoom(chatId, ChatSourceAction.ToggleReaction(messageId, emoji))?.let { return it }

        val existingUserReaction = messageReactions.find { it.origin.isUser() }
        if (existingUserReaction != null) removeUserReaction(chatId, existingUserReaction)
        if (existingUserReaction?.content?.emoji != emoji) addUserReaction(chatId, messageId, ChatMessageReactionContent(emoji))
        return ChatSourceActionOutcome.Done
    }

    /** Tells the product of a product-source room that the user saw the message. The host database needs nothing. */
    suspend fun reportMessageSeen(chatId: ChatId, messageId: ChatMessageId) {
        performInProductRoom(chatId, ChatSourceAction.MarkSeen(setOf(messageId)))
    }

    suspend fun sendAttachmentMessage(
        chatId: ChatId,
        contentUri: Uri,
        mimeType: String,
        fileSize: InformationSize,
        text: String?,
        replyToMessageId: ChatMessageId?
    ): Result<ChatSourceActionOutcome> {
        if (roomPolicyProvider.observePolicy(chatId).first() != null) {
            return sendProductAttachment(chatId, contentUri, mimeType, fileSize, text, replyToMessageId)
        }

        return sendHostedAttachment(chatId, contentUri, mimeType, fileSize, text, replyToMessageId).map { ChatSourceActionOutcome.Done }
    }

    /**
     * An attachment in a product room goes inline, up to 1 MiB, as rich text with one media item (02-DESIGN §3.5). The
     * product of a product-source room gets it as `NewMessage`; the product of a host-store room gets `MessagePosted`.
     */
    private suspend fun sendProductAttachment(
        chatId: ChatId,
        contentUri: Uri,
        mimeType: String,
        fileSize: InformationSize,
        text: String?,
        replyToMessageId: ChatMessageId?
    ): Result<ChatSourceActionOutcome> = productAttachmentEncoder.encode(contentUri, mimeType, fileSize).map { media ->
        val content = ChatMessage.Content.ProductBody(ProductMessageBody.RichText(text = text, media = listOf(media)))

        performInProductRoom(chatId, ChatSourceAction.Send(content, replyToMessageId)) ?: run {
            chatEngine.sendUserMessage(chatId = chatId, content = content, replyToMessageId = replyToMessageId)
            ChatSourceActionOutcome.Done
        }
    }

    private suspend fun sendHostedAttachment(
        chatId: ChatId,
        contentUri: Uri,
        mimeType: String,
        fileSize: InformationSize,
        text: String?,
        replyToMessageId: ChatMessageId?
    ) = runCatching {
        if (fileSize > MAX_ATTACHMENT_SIZE) {
            throw FileTooLargeException(fileSize, MAX_ATTACHMENT_SIZE)
        }

        // we have to make a copy since this uri will become unavailable once user closes the app
        // this process is quick, so we can do that right when we received original uri
        val localUri = attachmentFileStorage.copyToLocalStorage(contentUri, mimeType)

        val meta = attachmentMetaBuilder.build(localUri, mimeType, fileSize)
        val nodeUrl = hopNodeUrlProvider.pickForSending()
        val attachment = Attachment.Embedded(uri = localUri, meta = meta)
        val content = ChatMessage.Content.RichText(text = text, attachments = listOf(attachment))

        val message = chatEngine.sendAttachment(
            chatId = chatId,
            content = content,
            replyToMessageId = replyToMessageId
        )

        val upload = FileUpload.new(
            messageId = message.id,
            chatId = message.chatId,
            fileUri = localUri,
            mimeType = mimeType,
            nodeUrl = nodeUrl
        )

        fileUploadRepository.addUploadToQueue(upload)

        fileUploadStarter.startUploading()
    }

    suspend fun sendContactRequest(
        request: OpenChatRequest.StartChatWithContact,
        welcomeMessage: String,
    ): Result<Unit> {
        return with(request) {
            addContactUseCase.addContactWithChatRequest(
                contactAccountId = contactAccountId,
                username = username,
                avatar = avatar,
                chatKey = chatKey,
                sharedSecretDerivationDomain = sharedSecretDerivationDomain,
                ourMetaAccountId = ourMetaAccountId,
                origin = origin,
                welcomeMessage = ChatMessage.Content.RichText(welcomeMessage, emptyList())
            )
        }
    }

    suspend fun markMessagesAsReadUpToTimestamp(chatId: ChatId, timestamp: Timestamp) {
        chatMessageRepository.markMessagesAsReadUpToTimestamp(chatId, timestamp)
        chatNotificationPublisher.cancelChatNotification(chatId)
        chatEngine.reportSeen(chatId, timestamp)
    }

    /**
     * Holds the worker of a product room while the room screen shows it, and tells the product. A room that
     * cannot open shows "unavailable". A person chat holds nothing. The open and close calls keep their order.
     */
    fun openRoom(chatId: ChatId) = roomSource(chatId).open()

    /** Closes the room, which also clears its activity line (02-DESIGN §6.6). */
    fun closeRoom(chatId: ChatId) {
        activityWriter.set(chatId, null)
        roomSource(chatId).close()
    }

    /** The activity line of a product room, for example "Bot is typing". */
    fun observeRoomActivity(chatId: ChatId): Flow<ChatRoomActivityModel?> = activityRepository.observe(chatId)

    /** Opens the room again when the last open failed, then asks the product for the newest page. */
    fun retryRoom(chatId: ChatId) = roomSource(chatId).retry()

    /** The page and user-action queries of a room whose messages the product keeps, or null without a product. */
    suspend fun roomQuerying(chatId: ChatId): ProductChatQuerying? = chatEngine.roomQuerying(chatId)

    /**
     * The menu that the product gives for [message] of the window [snapshot] (`message_actions_query`). A failure is
     * a refusal, no answer in time, or a room without a product: the room screen then shows the host menu.
     */
    suspend fun productMessageMenu(chatId: ChatId, message: ChatMessage, snapshot: ChatFeedSnapshot): Result<ChatMessageMenuModel> {
        val querying = roomQuerying(chatId) ?: return Result.failure(ProductChatQueryException.NotAvailable())
        return querying.messageMenu(chatId, snapshot.productQueryMessage(message))
    }

    /** The address of the App of the product of the room, or null when no product serves the room. */
    suspend fun productAppAddress(chatId: ChatId): String? = chatEngine.productAppAddress(chatId)

    /** A product action of a menu, the toolbar or a suggestion that the user selected. */
    suspend fun sendUserEvent(chatId: ChatId, event: ChatRoomUserEvent): Result<Unit> =
        productRoomUserActions.sendEvent(chatId, event)

    // Workaround: messages like reactions and edits exist in DB but not in UI models,
    // causing the unread counter to be wrong. Mark them as read directly.
    suspend fun markNonDisplayableMessagesAsRead(chatId: ChatId) {
        val nonDisplayableTypes = listOf(
            ChatMessageLocal.Type.REACTED,
            ChatMessageLocal.Type.REACTION_REMOVED,
            ChatMessageLocal.Type.EDITED,
            ChatMessageLocal.Type.LEFT_CHAT,
        ).map { it.name }
        chatMessageRepository.markMessagesByTypesAsRead(chatId, nonDisplayableTypes)
    }

    suspend fun markMessagesAsRead(messageIds: List<ChatMessageId>) {
        chatMessageRepository.updateMessagesStatus(messageIds, ChatMessage.Status.IS_READ)
    }

    fun setChatActive(chatId: ChatId) {
        chatActiveTracker.setActive(chatId)
    }

    fun setChatInactive() {
        chatActiveTracker.clear()
    }

    fun getChatConfig(chatRequest: OpenChatRequest): Flow<ChatConfig> {
        return when (chatRequest) {
            is OpenChatRequest.ExistingChat -> chatEngine.getChatConfig(chatRequest.chatId)
            is OpenChatRequest.StartChatWithContact -> flowOfValue(ChatConfig.Default)
        }
    }

    suspend fun getFooterRendererForChat(chatId: ChatId): CustomChatFooterRenderer? =
        chatEngine.getFooterRendererForChat(chatId)

    suspend fun getMenuRendererForChat(chatId: ChatId): CustomChatMenuRenderer? =
        chatEngine.getMenuRendererForChat(chatId)

    suspend fun getCustomAppearanceForChat(chatId: ChatId): CustomChatAppearance? =
        chatEngine.getCustomAppearanceForChat(chatId)

    suspend fun animatesMessageReveal(chatId: ChatId): Boolean =
        chatEngine.animatesMessageReveal(chatId)

    suspend fun getLastRevealedTimestamp(chatId: ChatId): Timestamp? =
        chatEngine.getLastRevealedTimestamp(chatId)

    suspend fun setLastRevealedTimestamp(chatId: ChatId, timestamp: Timestamp) =
        chatEngine.setLastRevealedTimestamp(chatId, timestamp)

    suspend fun getCustomMessageRenderersForChat(chatId: ChatId): Map<String, CustomChatMessageRenderer<*>> =
        chatEngine.getCustomRenderersForChat(chatId)

    suspend fun blockUser(chatId: ChatId): Result<Unit> = runCatching {
        chatId.onContact { contactChat ->
            contactsRepository.setBlocked(contactChat.contactAccountId, true)
        }
    }

    suspend fun unblockUser(chatId: ChatId): Result<Unit> = runCatching {
        chatId.onContact { contactChat ->
            contactsRepository.setBlocked(contactChat.contactAccountId, false)
        }
    }

    suspend fun leaveChat(chatId: ChatId): Result<Unit> {
        tryNotifyPeerLeftChat(chatId)

        return deleteContactChatLocalData(chatId)
    }

    suspend fun sendEdit(chatId: ChatId, messageId: ChatMessageId, newText: String): ChatSourceActionOutcome {
        performInProductRoom(chatId, ChatSourceAction.Edit(messageId, ChatMessage.Content.Text(newText)))?.let { return it }

        chatEngine.editMessage(chatId, messageId, newText)
        return ChatSourceActionOutcome.Done
    }

    suspend fun getEditHistory(chatId: ChatId, messageId: ChatMessageId): List<MessageEditHistoryItem> {
        return chatEngine.getEditHistory(chatId, messageId)
    }

    suspend fun acceptIncomingRequest(contactAccountId: AccountId): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            contactsRepository.getContactResult(contactAccountId)
                .mapError { ChatRequestError.ContactNotFound }
                .flatMap { incomingChatRequestProcessor.acceptIncomingRequest(it) }
        }
    }

    suspend fun declineIncomingRequest(contactAccountId: AccountId): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            contactsRepository.getContactResult(contactAccountId)
                .mapError { ChatRequestError.ContactNotFound }
                .flatMap { incomingChatRequestProcessor.declineIncomingRequest(it) }
        }
    }

    private suspend fun tryNotifyPeerLeftChat(chatId: ChatId) {
        val contactId = chatId.contactOrNull()?.contactAccountId ?: return

        chatSessionManager.getSession(contactId) ?: return

        runCancellableCatching {
            val message = chatEngine.sendUserMessage(
                chatId = chatId,
                content = ChatMessage.Content.LeftChat
            )

            withTimeout(LEFT_CHAT_SENT_TIMEOUT) {
                chatMessageRepository.subscribeMessageStatus(message.id)
                    .first { it == ChatMessage.Status.IS_SENT || it == ChatMessage.Status.IS_READ }
            }
        }.logFailure("Failed to notify peer about leaving chat")
    }

    private fun getChatDisplay(startNewContact: OpenChatRequest.StartChatWithContact): ChatDisplay {
        return chatDisplayGenerator.generateAccountChatDisplay(
            accountId = startNewContact.contactAccountId,
            username = startNewContact.username?.getDisplayUsername(),
            avatarUrl = startNewContact.avatar
        )
    }

    private fun subscribeUserInputState(chatId: ChatId): Flow<ChatUserInputState> {
        return when (val variant = chatId.chatVariant()) {
            is ChatVariant.Extension -> chatEngine.subscribeUserInputStateChat(chatId)
            is ChatVariant.Contact -> subscribeUserInputStateForContact(variant.contactAccountId)
        }
    }

    private fun subscribeUserInputStateForContact(contactId: AccountId): Flow<ChatUserInputState> {
        return contactsRepository.subscribeContactWithChatRequest(contactId).map { contactWithRequest ->
            when {
                contactWithRequest == null -> ChatUserInputState.SendChatRequest

                contactWithRequest.pendingChatRequest != null && contactWithRequest.pendingChatRequest?.isOutgoing() == true -> ChatUserInputState.WaitChatRequestApproval

                contactWithRequest.pendingChatRequest != null && contactWithRequest.pendingChatRequest?.userDeclinedIncomingRequest() == true -> ChatUserInputState.UserDeclinedChatRequest

                contactWithRequest.pendingChatRequest != null -> ChatUserInputState.AcceptChatRequest

                contactWithRequest.contact.isBlocked -> ChatUserInputState.UnblockUser

                contactWithRequest.contact.isPeerLeft -> ChatUserInputState.PeerLeft

                else -> ChatUserInputState.SendMessage(
                    paymentState = contactWithRequest.contact.constructUserInputStatePayment(),
                    attachmentsSupported = true
                )
            }
        }
    }

    private fun Contact.constructUserInputStatePayment(): ChatUserInputState.SendMessage.Payment {
        val paymentAvailable = chatEngine.findOriginConfiguration(origin)?.paymentAvailable() ?: true
        return if (paymentAvailable) {
            ChatUserInputState.SendMessage.Payment.Available(accountId)
        } else {
            ChatUserInputState.SendMessage.Payment.NotAvailable
        }
    }

    private suspend fun deleteContactChatLocalData(chatId: ChatId): Result<Unit> = runCatching {
        deleteLocalMessages(chatId)
        deleteLocalContact(chatId)
        chatDraftRepository.deleteDraft(chatId)
    }

    private suspend fun deleteLocalMessages(chatId: ChatId) {
        chatMessageRepository.deleteAllChatMessages(chatId)
    }

    private suspend fun deleteLocalContact(chatId: ChatId) {
        chatId.onContact { contactChat ->
            removeContactUseCase.removeContact(contactChat.contactAccountId)
        }
    }

    fun observeRoomPolicy(chatId: ChatId): Flow<ChatRoomPolicy?> = roomPolicyProvider.observePolicy(chatId)

    suspend fun triggerMessageAction(chatId: ChatId, messageId: ChatMessageId, actionId: String): Result<Unit> =
        productRoomUserActions.sendEvent(chatId, ChatRoomUserEvent.ActionTriggered(messageId, actionId))

    suspend fun deleteProductRoomMessage(chatId: ChatId, messageId: ChatMessageId, policy: ChatRoomPolicy): Result<ChatSourceActionOutcome> {
        performInProductRoom(chatId, ChatSourceAction.Delete(messageId))?.let { return Result.success(it) }

        return productRoomUserActions.deleteMessage(chatId, messageId, policy).map { ChatSourceActionOutcome.Done }
    }

    suspend fun retryProductRoomMessage(chatId: ChatId, messageId: ChatMessageId): Result<ChatSourceActionOutcome> {
        performInProductRoom(chatId, ChatSourceAction.Retry(messageId))?.let { return Result.success(it) }

        return productRoomUserActions.retry(chatId, messageId).map { ChatSourceActionOutcome.Done }
    }

    /** The outcome of the action in a product-source room, or null when the host database keeps the messages. */
    private suspend fun performInProductRoom(chatId: ChatId, action: ChatSourceAction): ChatSourceActionOutcome? =
        roomSource(chatId).productSource()?.perform(action)

    private fun roomSource(chatId: ChatId): ChatRoomSourceContext =
        roomSources.computeIfAbsent(chatId) { roomSourceContextFactory.create(it) }

    companion object {
        private val LEFT_CHAT_SENT_TIMEOUT = 15.seconds
    }
}
