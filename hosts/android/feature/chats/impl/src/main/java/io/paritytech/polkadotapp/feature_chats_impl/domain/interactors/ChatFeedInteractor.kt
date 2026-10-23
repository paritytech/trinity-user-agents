package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import android.net.Uri
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.OpenChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isUser
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPublisher
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.ProductAttachmentEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatActiveTrackerInternal
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
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
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.withOutbox
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPaymentStore
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.withPayments
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductContactInput
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductRoomUserActions
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import kotlinx.coroutines.flow.flowOf as flowOfValue

class ChatFeedInteractor @Inject internal constructor(
    private val chatMessageRepository: ChatMessageRepository,
    private val chatNotificationPublisher: ChatNotificationPublisher,
    private val chatActiveTracker: ChatActiveTrackerInternal,
    private val chatEngine: ChatEngine,
    private val callController: CallController,
    private val callStateTracker: CallStateTracker,
    private val roomPolicyProvider: ChatRoomPolicyProvider,
    private val productRoomUserActions: ProductRoomUserActions,
    private val roomSourceContextFactory: ChatRoomSourceContextFactory,
    private val productAttachmentEncoder: ProductAttachmentEncoder,
    private val attachmentStager: ProductAttachmentStager,
    private val personChatRooms: PersonChatRooms,
    private val productPayments: ProductChatPaymentStore,
) {
    private val roomSources = ConcurrentHashMap<ChatId, ChatRoomSourceContext>()

    suspend fun initiateCall(chatId: ChatId, callerName: String, withVideo: Boolean): InitiateCallResult {
        val activeCall = callStateTracker.getActiveCall()

        return when {
            // As in a native chat, the call screen opens at once. The product of a person chat places the call.
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
        }
    }

    fun observeChatDisplay(request: OpenChatRequest): Flow<Result<ChatDisplay>> {
        return when (request) {
            is OpenChatRequest.ExistingChat -> chatEngine.observeChatDisplay(request.chatId)
        }.inBackground()
    }

    suspend fun getMessageOriginDisplayResolver(request: OpenChatRequest): MessageOriginDisplayResolver {
        return when (request) {
            is OpenChatRequest.ExistingChat -> chatEngine.originDisplayResolver(request.chatId)
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

    /**
     * The window of messages of the room, from the host database or from the product (02-DESIGN §8.3). The attachments
     * that the host stages show at the newest end until the host sends them (07-LATER §7). A payment shows the status
     * that the host keeps for it.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    fun subscribeMessages(chatId: ChatId): Flow<ChatFeedSnapshot> = combine(
        roomSource(chatId).observeSource().flatMapLatest { it.observeSnapshots() },
        attachmentStager.observe(chatId),
        productPayments.observeRoom(chatId),
    ) { snapshot, outbox, payments -> snapshot.withOutbox(outbox, attachmentStager::preview).withPayments(payments) }

    suspend fun loadOlderMessages(chatId: ChatId): ChatPageOutcome = roomSource(chatId).source().loadOlder()

    suspend fun loadNewerMessages(chatId: ChatId): ChatPageOutcome = roomSource(chatId).source().loadNewer()

    /** Replaces the window with the newest page of the room. */
    suspend fun showNewestMessages(chatId: ChatId) = roomSource(chatId).source().reload()

    /** Pages until the message is in the window. False when the room has no such message. */
    suspend fun jumpToMessage(chatId: ChatId, messageId: ChatMessageId): Boolean = roomSource(chatId).source().jump(messageId)

    /** True when the product of the room keeps its messages, and the host only a page cache. */
    suspend fun isProductSource(chatId: ChatId): Boolean = roomSource(chatId).productSource() != null

    /** Stops the source of the room and closes the room when it is open. Call it when the room screen goes away. */
    fun releaseRoom(chatId: ChatId) {
        roomSources.remove(chatId)?.release()
    }

    suspend fun sendTextMessage(
        chatId: ChatId,
        text: String,
        replyToMessageId: ChatMessageId?,
        request: Boolean = false,
    ): ChatSourceActionOutcome {
        val content = ChatMessage.Content.Text(text)

        // As the native chat saves a message before it sends it, a product-source room keeps it in the outbox, under the
        // UUID that the host chose, until the product has it.
        if (roomPolicyProvider.observePolicy(chatId).first()?.messageStore == ChatRoomPolicy.MessageStore.PRODUCT) {
            attachmentStager.enqueueText(chatId, text, replyToMessageId, request)
            performInProductRoom(chatId, ChatSourceAction.ShowNewest)
            return ChatSourceActionOutcome.Done
        }

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
        // A message that waits in the outbox takes the reaction now and sends it after the message.
        if (attachmentStager.toggleReactionPending(messageId, emoji)) return ChatSourceActionOutcome.Done
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
        // Only a room of the chat product sends its attachments to HOP; no other room takes attachments.
        val policy = roomPolicyProvider.observePolicy(chatId).first()
            ?.takeIf { it.contact != null || it.messageStore == ChatRoomPolicy.MessageStore.PRODUCT }
            ?: return Result.failure(IllegalStateException("The room takes no attachments"))
        return sendProductAttachment(chatId, policy, contentUri, mimeType, fileSize, text, replyToMessageId)
    }

    /**
     * An attachment in a product room goes to a HOP node of the person chat, as the native chat sends one. It waits in
     * the outbox, and a background job uploads it and then sends it (07-LATER §7).
     */
    private suspend fun sendProductAttachment(
        chatId: ChatId,
        policy: ChatRoomPolicy,
        contentUri: Uri,
        mimeType: String,
        fileSize: InformationSize,
        text: String?,
        replyToMessageId: ChatMessageId?
    ): Result<ChatSourceActionOutcome> {
        if (!policy.features.hopAttachments) {
            return Result.failure(IllegalStateException("The room takes no attachments"))
        }

        return productAttachmentEncoder.prepare(contentUri, mimeType, fileSize).map { draft ->
            attachmentStager.enqueue(chatId, draft, text, replyToMessageId)
            performInProductRoom(chatId, ChatSourceAction.ShowNewest)
            ChatSourceActionOutcome.Done
        }
    }

    suspend fun markMessagesAsReadUpToTimestamp(chatId: ChatId, timestamp: Timestamp) {
        chatMessageRepository.markMessagesAsReadUpToTimestamp(chatId, timestamp)
        chatNotificationPublisher.cancelChatNotification(chatId)
    }

    /**
     * Holds the worker of a product room while the room screen shows it, and tells the product. A room that
     * cannot open shows "unavailable". A person chat holds nothing. The open and close calls keep their order.
     */
    fun openRoom(chatId: ChatId) = roomSource(chatId).open()

    /** Sends again what waits in the outbox of the room, as the native chat does when a room opens. */
    suspend fun resumeOutbox(chatId: ChatId) = attachmentStager.resume(chatId)

    fun closeRoom(chatId: ChatId) = roomSource(chatId).close()

    fun observeRoomExists(chatId: ChatId): Flow<Boolean> = chatEngine.observeRoomExists(chatId)

    /** The page and user-action queries of a room whose messages the product keeps, or null without a product. */
    suspend fun roomQuerying(chatId: ChatId): ProductChatQuerying? = chatEngine.roomQuerying(chatId)

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

    suspend fun blockUser(chatId: ChatId): Result<Unit> = productRoomUserActions.contactAction(chatId, ChatUserActionModel.Block)

    suspend fun unblockUser(chatId: ChatId): Result<Unit> = productRoomUserActions.contactAction(chatId, ChatUserActionModel.Unblock)

    suspend fun leaveChat(chatId: ChatId): Result<Unit> = productRoomUserActions.contactAction(chatId, ChatUserActionModel.Leave)

    /** The answer of the user to the chat request of the contact of a room of the chat product. */
    suspend fun answerProductChatRequest(chatId: ChatId, accept: Boolean): Result<Unit> {
        val action = if (accept) ChatUserActionModel.AcceptRequest else ChatUserActionModel.DeclineRequest
        return productRoomUserActions.contactAction(chatId, action)
    }

    /** True for a room of the chat product, which the room screen draws as a person chat. */
    fun isPersonChatRoom(chatId: ChatId): Boolean = personChatRooms.isPersonChatRoom(chatId)

    /** The person a room of the chat product is a conversation with, or null for any other chat. */
    suspend fun contactOf(chatId: ChatId): ChatRoomContactModel? = roomPolicyProvider.observePolicy(chatId).first()?.contact

    suspend fun sendEdit(chatId: ChatId, messageId: ChatMessageId, newText: String): ChatSourceActionOutcome {
        // A message that waits in the outbox takes the edit now and sends it after the message.
        if (attachmentStager.editPending(messageId, newText, System.currentTimeMillis())) return ChatSourceActionOutcome.Done
        performInProductRoom(chatId, ChatSourceAction.Edit(messageId, ChatMessage.Content.Text(newText)))?.let { return it }

        chatEngine.editMessage(chatId, messageId, newText)
        return ChatSourceActionOutcome.Done
    }

    suspend fun getEditHistory(chatId: ChatId, messageId: ChatMessageId): List<MessageEditHistoryItem> {
        // A message that waits in the outbox has its first text and the edit that waits with it.
        attachmentStager.pending(messageId)?.let { entry ->
            val edited = entry.editedText ?: return emptyList()
            return listOf(MessageEditHistoryItem(entry.text.orEmpty(), entry.createdAt), MessageEditHistoryItem(edited, entry.editedAt ?: entry.createdAt))
        }
        // Only a room whose product keeps the messages knows their history; any other room has it in the database.
        val querying = roomQuerying(chatId)?.takeIf { isProductSource(chatId) } ?: return chatEngine.getEditHistory(chatId, messageId)

        return querying.editHistory(chatId, messageId)
            .logFailure("The product gave no edit history")
            .getOrNull()
            .orEmpty()
            .mapNotNull { revision -> revision.text?.let { MessageEditHistoryItem(it, revision.editedAt) } }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun subscribeUserInputState(chatId: ChatId): Flow<ChatUserInputState> {
        return when (chatId.chatVariant()) {
            is ChatVariant.Extension -> combine(roomPolicyProvider.observePolicy(chatId), attachmentStager.observe(chatId), ::Pair).flatMapLatest { (policy, outbox) ->
                // As in the native chat, the welcome message of a request turns the invite composer into the wait at once.
                val requestSent = policy?.contact?.state == ChatContactStateModel.INVITE && outbox.any { !it.isFailed }
                (if (requestSent) ChatUserInputState.WaitChatRequestApproval else ProductContactInput.of(policy))?.let(::flowOfValue)
                    // A room of the chat product shows no input of the product before its contact is declared.
                    ?: if (isPersonChatRoom(chatId)) flowOfValue(ChatUserInputState.Nothing) else chatEngine.subscribeUserInputStateChat(chatId)
            }
            // A contact chat id only names the account; its room is the one of the chat product.
            is ChatVariant.Contact -> flowOfValue(ChatUserInputState.Nothing)
        }
    }

    fun observeRoomPolicy(chatId: ChatId): Flow<ChatRoomPolicy?> = roomPolicyProvider.observePolicy(chatId)

    /** The outcome of the action in a product-source room, or null when the host database keeps the messages. */
    private suspend fun performInProductRoom(chatId: ChatId, action: ChatSourceAction): ChatSourceActionOutcome? =
        roomSource(chatId).productSource()?.perform(action)

    private fun roomSource(chatId: ChatId): ChatRoomSourceContext =
        roomSources.computeIfAbsent(chatId) { roomSourceContextFactory.create(it) }
}
