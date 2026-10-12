package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.common.utils.withFlowScope
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getAccountByIdOrThrow
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomResult
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.NewMessagesRoomFilter
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderersById
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOpenHandler
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDelegatesById
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomPreviewData
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.MessageDeliveryDelay
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.delayDelivery
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageDirection
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatOriginCustomConfiguration
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageProcessingRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MatchRendererCustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MessageRevisionRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.getContactResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.getExtensionForChatResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.observeAllExtensionsById
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.LastMessageSummary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageEditHistoryItem
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.chatSummaryOrder
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.DmChatMessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.reflect.KClass
import kotlinx.coroutines.flow.flowOf as flowOfValue

typealias CustomPreviewWithRenderer = Pair<ChatPreview.Custom<*>, CustomChatPreviewRenderer<*>>

@Singleton
class ChatEngine @Inject constructor(
    private val chatMessageRepository: ChatMessageRepository,
    private val chatMessageProcessingRepository: ChatMessageProcessingRepository,
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val accountRepository: AccountRepository,
    private val contactsRepository: ContactsRepository,
    private val messageRevisionRepository: MessageRevisionRepository,
    private val dmChatMessageOriginDisplayResolverFactory: DmChatMessageOriginDisplayResolver.Factory,
    private val chatDisplayGenerator: ChatDisplayGenerator,
    private val messageSaveProcessors: Set<@JvmSuppressWildcards ChatMessageSaveProcessor>,
    private val headerRenderers: Map<String, @JvmSuppressWildcards CustomChatHeaderRenderer>,
    private val originConfigurations: Map<String, @JvmSuppressWildcards ChatOriginCustomConfiguration>,
    private val chatRoomRepository: ChatRoomRepository,
    private val deleteRoomUseCase: DeleteRoomUseCase,
) : ChatMessageSender {
    private val savedMessagesFlow = MutableSharedFlow<ChatMessage>(extraBufferCapacity = 64)

    private val messagesChangedFlow = MutableSharedFlow<Unit>(extraBufferCapacity = 64)

    suspend fun getChatDisplay(chatId: ChatId): Result<ChatDisplay> {
        val defaultDisplay = when (val variant = chatId.chatVariant()) {
            is ChatVariant.Extension -> getDefaultChatExtensionDisplay(chatId)
            is ChatVariant.Contact -> getContactChatDisplay(variant)
        }

        val roomMetadata = chatRoomRepository.getRoomMetadata(chatId)

        return defaultDisplay.map { chatDisplayGenerator.applyRoomMetadata(roomMetadata, it) }
    }

    fun observeChatDisplay(chatId: ChatId): Flow<Result<ChatDisplay>> {
        return when (val variant = chatId.chatVariant()) {
            is ChatVariant.Contact -> contactsRepository.subscribeContact(variant.contactAccountId)
                .map { contact ->
                    Result.success(contact)
                        .requireNotNull { error("Contact with accountId=${variant.contactAccountId} not found") }
                        .map(::getContactChatDisplay)
                }

            is ChatVariant.Extension -> flowOf {
                val roomMetadata = chatRoomRepository.getRoomMetadata(chatId)
                getDefaultChatExtensionDisplay(chatId)
                    .map { chatDisplayGenerator.applyRoomMetadata(roomMetadata, it) }
            }
        }
    }

    fun getContactChatDisplay(contact: Contact): ChatDisplay {
        return chatDisplayGenerator.generateAccountChatDisplay(
            accountId = contact.accountId,
            username = contact.username,
            avatarUrl = contact.avatarUrl
        )
    }

    fun subscribeUserInputStateChat(chatId: ChatId): Flow<ChatUserInputState> {
        return flowOfAll {
            val extensionId = chatId.extensionOrNull()?.extensionId ?: return@flowOfAll flowOfValue(ChatUserInputState.Nothing)
            val chatExtension = chatExtensionRegistry.getExtension(extensionId) ?: return@flowOfAll flowOfValue(ChatUserInputState.Nothing)
            chatExtension.observeChatUserInputState(chatId)
        }
    }

    private fun ChatExtension.observeChatUserInputState(chatId: ChatId): Flow<ChatUserInputState> {
        return observeUserInputAllowed(chatId).map { isInputAllowed ->
            if (isInputAllowed) {
                ChatUserInputState.SendMessage(
                    paymentState = ChatUserInputState.SendMessage.Payment.NotAvailable,
                    attachmentsSupported = false
                )
            } else ChatUserInputState.Nothing
        }
    }

    suspend fun getDefaultChatExtensionDisplay(chatId: ChatId): Result<ChatDisplay> {
        return chatExtensionRegistry.getExtensionForChatResult(chatId)
            .map { extension ->
                val defaultMetadata = extension.defaultRoomMetadata ?: return@map null

                val avatar = defaultMetadata.icon?.let(ChatAvatar::Url)
                    ?: ChatAvatar.Account(defaultMetadata.name, themeSeed = chatId.value.value)

                ChatDisplay(defaultMetadata.name, avatar)
            }.requireNotNull()
    }

    suspend fun originDisplayResolver(chatId: ChatId): MessageOriginDisplayResolver {
        val contactVariant = chatId.contactOrNull()
        val contact = contactVariant?.let { contactsRepository.getContact(it.contactAccountId) }
        val ourAccountId = getOurAccountId(contact)

        return dmChatMessageOriginDisplayResolverFactory.create(
            chatDisplay = getChatDisplay(chatId).getOrThrow(),
            ourAccountId = ourAccountId,
        )
    }

    private suspend fun getOurAccountId(contact: Contact?): AccountId {
        val ourAccount = if (contact != null) {
            accountRepository.getAccountByIdOrThrow(contact.ourMetaAccountId)
        } else {
            accountRepository.getWalletAccount()
        }

        return ourAccount.defaultAccountId()
    }

    fun subscribeReactions(chatId: ChatId): Flow<Map<ChatMessageId, List<ChatMessageReaction>>> {
        return chatMessageRepository.subscribeReactions(chatId).map { reactions ->
            reactions.groupBy { it.messageId }
        }.inBackground()
    }

    fun subscribeMessagesFeed(chatId: ChatId): Flow<List<ChatMessage>> {
        return flowOfAll {
            val customContentDecoder = getCustomContentDecoder(chatId)
            chatMessageRepository.subscribeMessages(chatId, customContentDecoder)
        }.inBackground()
    }

    fun subscribeOutgoingMessagesByStatus(
        chatId: ChatId,
        status: ChatMessage.Status,
    ): Flow<List<ChatMessage>> {
        return withFlowScope {
            val customContentDecoder = getCustomContentDecoder(chatId)

            chatMessageRepository.subscribeOutgoingMessagesByStatus(chatId, status, customContentDecoder)
        }.inBackground()
    }
    fun observeMessagesChanged(): Flow<Unit> = messagesChangedFlow

    fun subscribeMessages(
        chatId: ChatId,
        type: KClass<out ChatMessage.Content>,
        status: ChatMessage.Status,
        direction: ChatMessageDirection
    ): Flow<List<ChatMessage>> = withFlowScope {
        val customContentDecoder = getCustomContentDecoder(chatId)
        chatMessageRepository.subscribeMessages(
            chatId = chatId,
            type = type,
            status = status,
            direction = direction,
            customContentDecoder = customContentDecoder
        )
    }

    context(scope: ComputationalScope)
    fun subscribeCallSignaling(): Flow<List<ChatMessage>> {
        return observeGlobalContentDecoder().flatMapLatest { contentDecoder ->
            chatMessageRepository.subscribeCallSignalingMessages(contentDecoder)
        }.inBackground()
    }

    context(scope: ComputationalScope)
    fun subscribeChatSummaries(): Flow<List<ChatSummary>> {
        return observeGlobalContentDecoder().flatMapLatest { contentDecoder ->
            val allChatSummaries = chatMessageRepository.subscribeLastMessageSummaries(contentDecoder)
            val visibleRoomIds = allChatSummaries.map { summaries -> summaries.mapToSet { it.chatId } }
                .distinctUntilChanged()

            val customPreviews = visibleRoomIds.flatMapLatest { customPreviews(it) }

            combine(allChatSummaries, customPreviews) { summaries, previews ->
                summaries
                    .map { createChatSummary(it, previews[it.chatId]) }
                    .sortedWith(chatSummaryOrder)
            }
        }
            .inBackground()
    }

    private fun createChatSummary(
        messageSummary: LastMessageSummary,
        customPreview: CustomPreviewWithRenderer?,
    ): ChatSummary {
        val lastMessage = messageSummary.lastMessage
        val customChatPreview = customPreview?.first
        val defaultTimestamp = lastMessage?.timestamp ?: messageSummary.chatCreatedAt

        val timestamp = when (customChatPreview?.order) {
            Order.PinToTop -> System.currentTimeMillis()
            Order.ByTimestamp -> defaultTimestamp
            null -> defaultTimestamp
        }

        // When the custom preview defers to the last message (FromMessage) we drop it into a
        // regular ChatPreview.Message but keep the custom Order so pinning still works.
        val preview = when {
            customChatPreview != null && customChatPreview.data is CustomPreviewData.FromMessage -> {
                if (lastMessage != null) ChatPreview.Message(lastMessage, order = customChatPreview.order)
                else ChatPreview.EmptyChat
            }

            customChatPreview != null -> customChatPreview
            lastMessage != null -> ChatPreview.Message(lastMessage)
            else -> ChatPreview.EmptyChat
        }

        // Renderer only applies when we actually kept the Custom preview with a renderer payload.
        val customPreviewRenderer = if (preview is ChatPreview.Custom<*>) customPreview?.second else null

        return ChatSummary(
            timestamp = timestamp,
            lastMessageSortOrder = messageSummary.lastMessageSortOrder,
            chatId = messageSummary.chatId,
            badge = createBadge(messageSummary, customChatPreview),
            preview = preview,
            order = customChatPreview?.order ?: Order.ByTimestamp,
            roomMetadata = messageSummary.roomMetadata,
            hasUnseenReaction = messageSummary.hasUnseenReaction,
            customPreviewRenderer = customPreviewRenderer,
        )
    }

    private fun createBadge(messageSummary: LastMessageSummary, customPreview: ChatPreview.Custom<*>?): ChatSummaryBadge {
        return when (customPreview?.badgeStyle) {
            null, ChatPreview.Custom.BadgeStyle.UNREAD -> createUnseenBadge(messageSummary.unseenCount)
            ChatPreview.Custom.BadgeStyle.NOTIFICATION -> ChatSummaryBadge.Notification
            ChatPreview.Custom.BadgeStyle.NONE -> ChatSummaryBadge.None
        }
    }

    private fun createUnseenBadge(count: Int) = if (count > 0) {
        ChatSummaryBadge.Unread(count)
    } else {
        ChatSummaryBadge.None
    }

    context(scope: ComputationalScope)
    private fun customPreviews(roomIds: Set<ChatId>): Flow<Map<ChatId, CustomPreviewWithRenderer?>> {
        return observeAllCustomChatPreviewDelegates(roomIds)
            .flatMapLatest { delegatesMap ->
                if (delegatesMap.isEmpty()) return@flatMapLatest flowOfValue(emptyMap())

                val previewsByChatIdFlows = delegatesMap
                    .mapNotNull { (chatId, delegate) ->
                        delegate.provider.provide()
                            .onStart { emit(null) }
                            .catch {
                                emit(null)

                                Timber.e(it, "Failed to get preview data for ${chatId.chatVariant()}")
                            }
                            .distinctUntilChanged()
                            .map { customPreview ->
                                val pair = customPreview?.let { it to delegate.renderer }
                                chatId to pair
                            }
                    }

                combine(previewsByChatIdFlows) { it.toMap() }
            }
    }

    fun getChatConfig(chatId: ChatId): Flow<ChatConfig> {
        return flowOf { chatExtensionRegistry.getExtensionForChat(chatId) }
            .flatMapLatest { extension ->
                extension?.observeChatConfig(chatId) ?: flowOfValue(ChatConfig.Default)
            }
    }

    suspend fun awaitMessage(chatMessageId: ChatMessageId): ChatMessage {
        return observeGlobalContentDecoder()
            .flatMapLatest { chatMessageRepository.subscribeMessageById(chatMessageId, it) }
            .filterNotNull()
            .first()
    }

    context(scope: ComputationalScope)
    override fun startExtensions() {
        scope.launch {
            val globalDecoder = getGlobalContentDecoder()

            chatExtensionRegistry.getStaticExtensions().forEach { chatExtension ->
                val context = RealChatExtensionContext(scope, chatExtension.id, globalDecoder)
                with(context) { chatExtension.startGlobalWork() }
            }

            launchExternalExtensions(globalDecoder)
        }
    }

    context(scope: ComputationalScope)
    private fun launchExternalExtensions(
        globalDecoder: CustomContentDecoder,
    ) {
        val activeExtensions = mutableMapOf<String, ExternalExtension>()

        chatExtensionRegistry.observeExternalExtensions()
            .diffed()
            .onEach { diff ->
                diff.removed.forEach { extension ->
                    activeExtensions.remove(extension.identifier)?.dispose()
                }

                diff.updated.forEach { newExtension ->
                    activeExtensions.remove(newExtension.identifier)?.dispose()
                    val context = RealChatExtensionContext(scope, newExtension.id, globalDecoder)
                    with(context) { newExtension.startGlobalWork() }
                    activeExtensions[newExtension.identifier] = newExtension
                }

                diff.added.forEach { extension ->
                    val context = RealChatExtensionContext(scope, extension.id, globalDecoder)
                    with(context) { extension.startGlobalWork() }
                    activeExtensions[extension.identifier] = extension
                }
            }
            .launchIn(scope)
    }

    suspend fun sendAttachment(
        chatId: ChatId,
        content: ChatMessage.Content.RichText,
        replyToMessageId: String?
    ): ChatMessage {
        val message = ChatMessage.new(
            chatId = chatId,
            content = content,
            origin = ChatMessageOrigin.User,
            status = ChatMessage.Status.PROCESSING,
            replyToMessageId = replyToMessageId
        )
        saveMessage(message)
        return message
    }

    override suspend fun sendUserMessage(
        messageId: ChatMessageId,
        chatId: ChatId,
        content: ChatMessage.Content,
        replyToMessageId: String?,
        onSaved: suspend () -> Unit,
    ): ChatMessage {
        val initialStatus = determineInitialMessageStatus(chatId)
        val message = ChatMessage.new(
            messageId = messageId,
            chatId = chatId,
            content = content,
            origin = ChatMessageOrigin.User,
            status = initialStatus,
            replyToMessageId = replyToMessageId
        )
        saveMessage(message, onSaved = onSaved)
        return message
    }

    suspend fun sendBotMessage(botId: ChatExtensionId, content: ChatMessage.Content, deliveryDelay: MessageDeliveryDelay) {
        val chatId = ChatId.fromChatBotId(botId)

        val botMessage = ChatMessage.new(
            chatId = chatId,
            content = content,
            origin = ChatMessageOrigin.Extension(botId),
            status = ChatMessage.Status.IS_READ
        )
        deliveryDelay.delayDelivery()
        saveMessage(botMessage)
    }

    suspend fun getFooterRendererForChat(chatId: ChatId): CustomChatFooterRenderer? {
        return chatExtensionRegistry.getExtensionForChat(chatId)?.customFooterRenderer(chatId)
    }

    suspend fun getMenuRendererForChat(chatId: ChatId): CustomChatMenuRenderer? {
        return chatExtensionRegistry.getExtensionForChat(chatId)?.customMenuRenderer(chatId)
    }

    suspend fun getOpenHandlerForChat(chatId: ChatId): CustomChatOpenHandler? {
        return chatExtensionRegistry.getExtensionForChat(chatId)?.customOpenHandler(chatId)
    }

    suspend fun getCustomAppearanceForChat(chatId: ChatId): CustomChatAppearance? {
        return chatExtensionRegistry.getExtensionForChat(chatId)?.customChatAppearance(chatId)
    }

    suspend fun animatesMessageReveal(chatId: ChatId): Boolean {
        return chatExtensionRegistry.getExtensionForChat(chatId)?.animatesMessageReveal(chatId) ?: false
    }

    suspend fun getLastRevealedTimestamp(chatId: ChatId): Timestamp? {
        val extensionId = chatId.extensionOrNull()?.extensionId ?: return null
        return chatMessageProcessingRepository.getLastRevealedTimestamp(extensionId)
    }

    suspend fun setLastRevealedTimestamp(chatId: ChatId, timestamp: Timestamp) {
        val extensionId = chatId.extensionOrNull()?.extensionId ?: return
        chatMessageProcessingRepository.setLastRevealedTimestamp(extensionId, timestamp)
    }

    suspend fun getHeaderRendererForChat(chatId: ChatId): CustomChatHeaderRenderer? {
        val contactAccountId = chatId.contactOrNull()?.contactAccountId ?: return null
        val contact = contactsRepository.getContact(contactAccountId) ?: return null
        return headerRenderers[contact.origin]
    }

    fun findOriginConfiguration(origin: ContactOrigin) = originConfigurations[origin]

    suspend fun saveMessage(
        chatMessage: ChatMessage,
        onConflict: ChatMessageSaveConflictStrategy = ChatMessageSaveConflictStrategy.REPLACE,
        placement: ChatMessagePlacement = ChatMessagePlacement.Latest,
        onSaved: suspend () -> Unit = {},
    ): Boolean {
        val customContentDecoder = getCustomContentDecoder(chatMessage.chatId)
        return chatMessageRepository.saveMessage(chatMessage, customContentDecoder, onConflict, placement, onSaved)
            .also { saved ->
                if (saved) {
                    messageSaveProcessors.forEach { it.onMessageSaved(chatMessage) }
                    savedMessagesFlow.tryEmit(chatMessage)
                    messagesChangedFlow.tryEmit(Unit)
                }
            }
    }

    suspend fun saveMessages(
        chatMessages: List<ChatMessage>,
        onConflict: ChatMessageSaveConflictStrategy = ChatMessageSaveConflictStrategy.REPLACE,
        placement: ChatMessagePlacement,
    ): List<ChatMessage> {
        if (chatMessages.isEmpty()) return emptyList()

        val customContentDecoder = getGlobalContentDecoder()
        val savedMessages = chatMessageRepository.saveMessages(chatMessages, customContentDecoder, onConflict, placement)

        savedMessages.forEach { saved ->
            messageSaveProcessors.forEach { it.onMessageSaved(saved) }
            savedMessagesFlow.tryEmit(saved)
        }

        if (savedMessages.isNotEmpty()) {
            messagesChangedFlow.tryEmit(Unit)
        }

        return savedMessages
    }

    suspend fun markMessageAsRead(messageId: ChatMessageId) {
        chatMessageRepository.updateMessageStatus(messageId, ChatMessage.Status.IS_READ)
    }

    suspend fun updateMessageStatus(messageId: ChatMessageId, status: ChatMessage.Status) {
        chatMessageRepository.updateMessageStatus(messageId, status)
        messagesChangedFlow.tryEmit(Unit)
    }

    suspend fun updateMessageContent(
        chatId: ChatId,
        messageId: ChatMessageId,
        content: ChatMessage.Content
    ) {
        val contentDecoder = getCustomContentDecoder(chatId)
        chatMessageRepository.updateMessageContent(messageId, content, contentDecoder)
        messagesChangedFlow.tryEmit(Unit)
    }

    suspend fun getCustomRenderersForChat(chatId: ChatId): CustomChatMessageRenderersById {
        return chatExtensionRegistry.getExtensionForChat(chatId)
            ?.customMessageRenderers()
            ?.associateBy { it.id }
            ?: emptyMap()
    }

    fun observeActiveCustomMessageRenderers(): Flow<CustomChatMessageRenderersById> {
        return chatExtensionRegistry.observeActiveExtensions()
            .map { bots -> bots.messageRenderersById() }
    }

    fun observeActiveOverlays(): Flow<List<ChatOverlay>> {
        return chatExtensionRegistry.observeActiveExtensions()
            .map { extensions ->
                extensions.mapNotNull { ext ->
                    ext.customGlobalOverlayRenderer()?.let { renderer ->
                        ChatOverlay(
                            renderer = renderer,
                            ownedFragmentClasses = ext.ownedFragmentClasses(),
                        )
                    }
                }
            }
    }

    fun observeAllCustomChatPreviewDelegates(chatIds: Collection<ChatId>): Flow<CustomChatPreviewDelegatesById> {
        return chatExtensionRegistry.observeAllExtensionsById()
            .map { extensions ->
                chatIds.associateWith { chatId ->
                    val extensionId = chatId.extensionOrNull()?.extensionId ?: return@associateWith null
                    val extension = extensions[extensionId] ?: return@associateWith null

                    extension.customChatPreviewDelegate(chatId)
                }
                    .filterNotNull()
            }
    }

    suspend fun getAllCustomMessageRenderers(): CustomChatMessageRenderersById {
        return chatExtensionRegistry.getAllExtensions().messageRenderersById()
    }

    private fun List<ChatExtension>.messageRenderersById(): CustomChatMessageRenderersById {
        return flatMap { it.customMessageRenderers() }
            .associateBy { it.id }
    }

    private suspend fun getContactChatDisplay(chatId: ChatVariant.Contact): Result<ChatDisplay> {
        return contactsRepository.getContactResult(chatId.contactAccountId)
            .map(::getContactChatDisplay)
    }

    private fun determineInitialMessageStatus(chatId: ChatId): ChatMessage.Status {
        return when (chatId.chatVariant()) {
            is ChatVariant.Extension -> ChatMessage.Status.IS_READ
            is ChatVariant.Contact -> ChatMessage.Status.NEW
        }
    }

    fun subscribeRevisions(chatId: ChatId): Flow<Map<ChatMessageId, MessageRevision>> {
        return flowOfAll {
            val decoder = getCustomContentDecoder(chatId)
            messageRevisionRepository.subscribeRevisions(chatId, decoder)
        }
    }

    suspend fun saveRevision(
        messageId: ChatMessageId,
        content: ChatMessage.Content,
        chatId: ChatId,
        timestamp: Long
    ) {
        messageRevisionRepository.saveRevision(
            messageId = messageId,
            content = content,
            chatId = chatId,
            timestamp = timestamp,
            customContentDecoder = getCustomContentDecoder(chatId)
        )
    }

    suspend fun editMessage(chatId: ChatId, messageId: ChatMessageId, newText: String) {
        val decoder = getCustomContentDecoder(chatId)
        val oldMessage = chatMessageRepository.getMessageById(chatId, messageId, decoder)
        val attachments = (oldMessage?.content as? ChatMessage.Content.RichText)?.attachments.orEmpty()

        val timestamp = System.currentTimeMillis()
        val newContent = ChatMessage.Content.RichText(newText, attachments)

        saveRevision(
            messageId = messageId,
            content = newContent,
            chatId = chatId,
            timestamp = timestamp
        )

        sendUserMessage(
            chatId = chatId,
            content = ChatMessage.Content.Edited(messageId, newContent)
        )
    }

    suspend fun getEditHistory(chatId: ChatId, messageId: ChatMessageId): List<MessageEditHistoryItem> {
        val decoder = getCustomContentDecoder(chatId)

        val currentMessage = chatMessageRepository.getMessageById(chatId, messageId, decoder)

        val currentRevision = currentMessage?.let { message ->
            message.getText()?.let { MessageEditHistoryItem(it, message.timestamp) }
        } ?: return emptyList()

        val previousRevisions = messageRevisionRepository
            .getRevisionsForMessage(messageId, decoder)
            .mapNotNull {
                val text = it.getText() ?: return@mapNotNull null
                MessageEditHistoryItem(text, it.timestamp)
            }

        return previousRevisions + currentRevision
    }

    suspend fun getMessageById(chatId: ChatId, messageId: ChatMessageId): ChatMessage? {
        return chatMessageRepository.getMessageById(chatId, messageId, getCustomContentDecoder(chatId))
    }

    private fun ChatMessage.getText() = (content as? ChatMessage.Content.Text)?.text ?: (content as? ChatMessage.Content.RichText)?.text

    private fun MessageRevision.getText() = (content as? ChatMessage.Content.Text)?.text ?: (content as? ChatMessage.Content.RichText)?.text

    private suspend fun getCustomContentDecoder(chatId: ChatId): CustomContentDecoder {
        return MatchRendererCustomContentDecoder(getCustomRenderersForChat(chatId))
    }

    private suspend fun getGlobalContentDecoder(): CustomContentDecoder {
        return MatchRendererCustomContentDecoder(getAllCustomMessageRenderers())
    }

    private fun observeGlobalContentDecoder(): Flow<CustomContentDecoder> {
        return observeActiveCustomMessageRenderers().map(::MatchRendererCustomContentDecoder)
    }

    private inner class RealChatExtensionContext(
        override val scope: ComputationalScope,
        private val extensionId: ChatExtensionId,
        private val contentDecoder: CustomContentDecoder,
    ) : ChatExtensionContext {
        override suspend fun createRoom(request: CreateRoomRequest): CreateRoomResult {
            return chatRoomRepository.createRoom(request)
        }

        override suspend fun deleteRoom(chatId: ChatId) {
            deleteRoomUseCase.invoke(chatId)
        }

        override suspend fun updateRoomMetadata(chatId: ChatId, metadata: RoomMetadata) {
            chatRoomRepository.updateRoomMetadata(chatId, metadata)
        }

        override suspend fun sendMessage(chatId: ChatId, content: ChatMessage.Content): ChatMessage {
            return ChatMessage.new(
                chatId = chatId,
                content = content,
                origin = ChatMessageOrigin.Extension(extensionId),
            ).also {
                saveMessage(it)
            }
        }

        override suspend fun modifyMessage(chatId: ChatId, messageId: ChatMessageId, content: ChatMessage.Content) {
            chatMessageRepository.updateMessageContent(messageId, content, contentDecoder)
        }

        override suspend fun removeMessage(chatId: ChatId, messageId: ChatMessageId) {
            chatMessageRepository.removeMessage(messageId)
        }

        override suspend fun getPersistedMessages(chatId: ChatId): List<ChatMessage> {
            return chatMessageRepository.getMessages(chatId, contentDecoder)
        }

        override suspend fun setWelcomeMessages(chatId: ChatId, messageBuilder: () -> List<ChatMessage.Content>) {
            if (!chatMessageProcessingRepository.hasBotSentWelcomeMessage(extensionId)) {
                messageBuilder().forEach { content ->
                    sendMessage(chatId, content)
                }
                chatMessageProcessingRepository.markWelcomeMessageSent(extensionId)
            }
        }

        override fun subscribeNewMessages(
            roomFilter: NewMessagesRoomFilter,
            contentTypes: Collection<KClass<out ChatMessage.Content>>?,
        ): Flow<ChatMessage> {
            return savedMessagesFlow
                .filter { msg -> roomFilter.matches(msg.chatId) }
                .filter { msg -> contentTypes == null || contentTypes.any { it.isInstance(msg.content) } }
        }

        override suspend fun getUnprocessedMessages(
            roomIds: Collection<ChatId>?,
            contentTypes: Collection<KClass<out ChatMessage.Content>>?,
        ): List<ChatMessage> {
            val unprocessed = chatMessageProcessingRepository.getUnprocessedMessages(extensionId)

            return unprocessed
                .map { it.toDomain(contentDecoder) }
                .filter { msg -> roomIds == null || msg.chatId in roomIds.toSet() }
                .filter { msg -> contentTypes == null || contentTypes.any { it.isInstance(msg.content) } }
        }

        override suspend fun markMessageProcessed(chatId: ChatId, messageId: ChatMessageId) {
            chatMessageProcessingRepository.markMessageProcessedByExtension(
                extensionId = extensionId,
                messageId = messageId,
                chatId = chatId,
            )
        }

        override fun subscribeOwnRooms(): Flow<List<ChatId>> {
            return chatRoomRepository.subscribeRoomsByExtension(extensionId)
        }
    }
}

enum class ChatMessageSaveConflictStrategy {
    REPLACE,
    IGNORE
}

inline fun <reified T : ChatMessage.Content> ChatEngine.subscribeMessages(
    chatId: ChatId,
    status: ChatMessage.Status,
    direction: ChatMessageDirection
) = subscribeMessages(chatId, T::class, status, direction)
