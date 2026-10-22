package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.DefaultRoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.NewMessagesRoomFilter
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDelegate
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageRevisionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomLifecycle
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_api.model.productRoomId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageRenderer
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatRoomEventSink
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.WorkerModalityApi
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu.ProductChatMenuRenderer
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import timber.log.Timber

/**
 * Products extension that drives a product's shared worker. Supports multiple rooms — JS scripts can
 * create rooms via [ProductChatMessaging.createRoom].
 *
 * The worker itself is owned by [ProductWorkerRefCounter]; this extension is the only consumer that
 * drives it. It binds its [ProductChatMessaging] onto the shared worker while attached and releases
 * the driving lease on dispose.
 *
 * Each instance is created by [ProductBotFactory] for a specific [Product].
 *
 * A product the host placed ([ProductChatExtensionOrigin.PlacedByHost]) has its rooms pinned above
 * the rest and offers no removal.
 */
class ProductChatExtension(
    appContext: Context,
    val product: Product,
    private val workerRefCounter: ProductWorkerRefCounter,
    imageResolver: ProductImageResolver,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val feedListeners: ProductFeedListeners,
    private val roomEventSink: ProductChatRoomEventSink,
    private val callRepository: ChatRoomCallRepository,
    val origin: ProductChatExtensionOrigin = ProductChatExtensionOrigin.InstalledByUser,
) : ExternalExtension() {
    override val id = product.id.toChatExtensionId()

    override val activationStateExternallyControlled: Boolean = false

    override val defaultRoomMetadata: DefaultRoomMetadata = DefaultRoomMetadata(product.name, icon = null)

    private val runningWorker = DeferredProductWorker()
    private val messageRenderer = ProductsMessageRenderer(appContext, product, runningWorker, imageResolver)

    private class Disposed : Exception("the product's chat extension was disposed")

    private var botScope: CoroutineScope? = null

    // Strong ref to the messaging target bound weakly onto the shared worker; held for our lifetime.
    private var chatMessaging: ProductChatMessaging? = null

    // Rooms the product asked to show nothing below the messages. Absent means the text input, the default.
    private val roomsWithoutInput = MutableStateFlow(emptySet<ProductChatIdParameter>())

    override fun observeUserInputAllowed(chatId: ChatId): Flow<Boolean> {
        val room = chatId.productRoomId(id) ?: return flowOf(true)
        return roomsWithoutInput.map { room !in it }.distinctUntilChanged()
    }

    override fun customMessageRenderers(): List<CustomChatMessageRenderer<*>> {
        return listOf(messageRenderer)
    }

    override fun customMenuRenderer(chatId: ChatId): CustomChatMenuRenderer =
        ProductChatMenuRenderer(product, chatId, removable = origin == ProductChatExtensionOrigin.InstalledByUser)

    override fun customChatPreviewDelegate(chatId: ChatId): CustomChatPreviewDelegate<*> =
        ProductChatPreviewDelegate(origin, isPersonChat = declarationsRepository.observe(chatId).map { it?.contact != null })

    context(chatExtensionContext: ChatExtensionContext)
    override fun startGlobalWork() {
        val scope = chatExtensionContext.scope.childScope(supervised = true)
        botScope = scope

        val messaging = ChatMessagingBinding(chatExtensionContext)
        chatMessaging = messaging

        scope.launch {
            Timber.d("Starting a bot ${product.name}...")

            chatExtensionContext.subscribeNewMessages(NewMessagesRoomFilter.AnyFromExtension(id))
                .filter { it.origin !is ChatMessageOrigin.Extension }
                .onEach { message -> routeMessage(chatExtensionContext, message) }
                .launchIn(scope)

            // Enable chat messaging before the worker's started hook so an initial/welcome message
            // routes. The reference is released from the finally so a dispose that races boot never
            // leaks the acquisition.
            val reference = workerRefCounter.acquire(product.id, "chat:${product.id.value}")
            try {
                reference.enableModalityApi(WorkerModalityApi.Chat(messaging))
                // A boot failure must surface as failed calls, not as a chat that waits forever.
                runCatching { reference.worker() }.fold(
                    onSuccess = { worker ->
                        runningWorker.attach(worker)
                        // A product with no room yet has no screen to start it, so it runs a short while to create one.
                        val ownRooms = chatExtensionContext.subscribeOwnRooms().firstOrNull().orEmpty()
                        if (ownRooms.none { it.productRoomId(id) != null }) worker.holdBriefly()
                    },
                    onFailure = { error ->
                        if (error is CancellationException) throw error
                        Timber.e(error, "No worker to drive the chat of ${product.id.value}")
                        runningWorker.fail(error)
                    },
                )
                awaitCancellation()
            } finally {
                withContext(NonCancellable) { reference.release() }
                // A cell still held by a surviving view model must fail, not wait.
                runningWorker.fail(Disposed())
            }
        }
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is ProductChatExtension) return false
        return product == other.product && origin == other.origin
    }

    override fun hashCode(): Int = product.hashCode()

    override suspend fun dispose() {
        // Cancelling the scope runs the finally in startGlobalWork, which releases the worker lease.
        botScope?.cancel()
        botScope = null
        chatMessaging = null
        Timber.d("Disposed product bot: ${product.name}")
    }

    override fun roomLifecycle(chatId: ChatId): ChatRoomLifecycle? {
        if (chatId.productRoomId(id) == null) return null
        return object : ChatRoomLifecycle {
            override suspend fun open(chatId: ChatId): Result<Unit> {
                val room = chatId.productRoomId(id) ?: return Result.failure(UnknownChatRoomException())
                return runningWorker.openRoom(room)
            }

            override suspend fun close(chatId: ChatId) {
                chatId.productRoomId(id)?.let { runningWorker.closeRoom(it) }
            }
        }
    }

    override fun roomQuerying(chatId: ChatId): ProductChatQuerying? {
        if (chatId.productRoomId(id) == null) return null
        return object : ProductChatQuerying {
            override suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage> {
                val room = chatId.productRoomId(id) ?: return Result.failure(ProductChatQueryException.UnknownRoom())
                return runningWorker.messagesPage(room, anchor, pageSize)
            }

            override suspend fun userAction(chatId: ChatId, action: ChatUserActionModel): Result<ChatUserActionOutcome> {
                val room = chatId.productRoomId(id) ?: return Result.failure(ProductChatQueryException.UnknownRoom())
                return runningWorker.userAction(room, action.withProductIds(chatId))
            }

            override suspend fun editHistory(chatId: ChatId, messageId: String): Result<List<ChatMessageRevisionModel>> {
                val room = chatId.productRoomId(id) ?: return Result.failure(ProductChatQueryException.UnknownRoom())
                return runningWorker.editHistory(room, ProductMessageIds.product(messageId, chatId))
            }

            override suspend fun openLink(chatId: ChatId, payload: String): Result<ChatId?> {
                val room = chatId.productRoomId(id) ?: return Result.failure(ProductChatQueryException.UnknownRoom())
                return runningWorker.openLink(room, payload).map { it?.toChatId(id) }
            }

            override suspend fun search(chatId: ChatId, text: String, cursor: String?, pageSize: Int): Result<ProductMessagesPage> {
                val room = chatId.productRoomId(id) ?: return Result.failure(ProductChatQueryException.UnknownRoom())
                return runningWorker.search(room, text, cursor, pageSize)
            }
        }
    }

    // The room screen may name a message by its store id; the product knows it by its own id.
    private fun ChatUserActionModel.withProductIds(chatId: ChatId): ChatUserActionModel {
        fun productId(id: String) = ProductMessageIds.product(id, chatId)
        return when (this) {
            is ChatUserActionModel.NewMessage -> copy(replyTo = replyTo?.let(::productId))
            is ChatUserActionModel.EditMessage -> copy(messageId = productId(messageId))
            is ChatUserActionModel.ReactionPlaced -> copy(messageId = productId(messageId))
            is ChatUserActionModel.ReactionRemoved -> copy(messageId = productId(messageId))
            is ChatUserActionModel.Seen -> copy(messageId = productId(messageId))
            ChatUserActionModel.AcceptRequest,
            ChatUserActionModel.DeclineRequest,
            ChatUserActionModel.Block,
            ChatUserActionModel.Unblock,
            ChatUserActionModel.Leave,
            -> this
        }
    }

    // The core keeps the event until the worker subscribes, so the event does not start the worker.
    override suspend fun onRoomEvent(chatId: ChatId, event: ChatRoomEventModel) {
        val room = chatId.productRoomId(id) ?: return
        roomEventSink.publish(product.id, room, event)
            .onFailure { Timber.w(it, "No room event %s for %s", event::class.simpleName, product.id.value) }
    }

    // In a room whose product keeps the messages, the user's messages go to `user_action`.
    private suspend fun routeMessage(context: ChatExtensionContext, message: ChatMessage) {
        val config = context.roomDeclarations(message.chatId)?.effectiveConfig ?: ChatRoomConfigModel.HOST_DEFAULT
        if (config.productSource) return
        when (val content = message.content) {
            is ChatMessage.Content.Text -> runningWorker.onUserMessage(message.chatId.productRoomId(id), content.text)
            else -> {}
        }
    }

    private inner class ChatMessagingBinding(
        private val extensionContext: ChatExtensionContext,
    ) : ProductChatMessaging {
        override suspend fun createRoom(request: CreateProductRoomRequest): Result<CreateProductRoomResult> {
            val chatId = request.chatIdParameter.toChatId(id)
            val roomRequest = CreateRoomRequest(chatId, request.name, request.icon)
            val response = extensionContext.createRoom(roomRequest)
            return Result.success(CreateProductRoomResult(response.status))
        }

        override suspend fun sendMessage(chatIdParameter: ProductChatIdParameter, message: ProductBotMessage): Result<ChatMessageId> {
            val chatId = chatIdParameter.toChatId(id)
            val chatMessage = extensionContext.sendMessage(chatId, message.toChatMessageContent())
            return Result.success(chatMessage.id)
        }

        override suspend fun applyFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Result<Unit> =
            extensionContext.applyFeedUpdate(chatIdParameter.toChatId(id), update)

        override suspend fun deliverLiveFeedUpdate(chatIdParameter: ProductChatIdParameter, update: ProductFeedUpdateModel): Boolean =
            feedListeners.deliver(chatIdParameter.toChatId(id), update)

        override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> {
            return extensionContext.subscribeOwnRooms().map { chatIds ->
                chatIds.mapNotNull { chatId ->
                    chatId.productRoomId(id)?.let { ProductChatRoom(roomId = it.value, participatingAs = RoomParticipation.ROOM_HOST) }
                }
            }
        }

        override suspend fun roomDeclarations(chatIdParameter: ProductChatIdParameter): Result<ChatRoomDeclarations?> =
            runCancellableCatching { extensionContext.roomDeclarations(chatIdParameter.toChatId(id)) }

        override suspend fun setRoomConfig(chatIdParameter: ProductChatIdParameter, config: ChatRoomConfigModel): Result<Unit> =
            extensionContext.setRoomConfig(chatIdParameter.toChatId(id), config)

        override suspend fun setRoomMetadata(
            chatIdParameter: ProductChatIdParameter,
            name: String,
            metadata: ChatRoomMetadataModel,
        ): Result<Unit> = extensionContext.setRoomMetadata(chatIdParameter.toChatId(id), name, metadata)

        override suspend fun setRoomPreview(chatIdParameter: ProductChatIdParameter, preview: ChatRoomPreviewModel): Result<Unit> =
            extensionContext.setRoomPreview(chatIdParameter.toChatId(id), preview)

        override suspend fun deleteRoom(chatIdParameter: ProductChatIdParameter): Result<Unit> =
            runCancellableCatching { extensionContext.deleteRoom(chatIdParameter.toChatId(id)) }

        override suspend fun setRoomFooter(chatIdParameter: ProductChatIdParameter, showsTextInput: Boolean): Result<Unit> {
            roomsWithoutInput.update { rooms ->
                if (showsTextInput) rooms - chatIdParameter else rooms + chatIdParameter
            }
            return Result.success(Unit)
        }

        override suspend fun setRoomFeatures(chatIdParameter: ProductChatIdParameter, features: ChatRoomFeaturesModel): Result<Unit> =
            extensionContext.setRoomFeatures(chatIdParameter.toChatId(id), features)

        override suspend fun setRoomCall(chatIdParameter: ProductChatIdParameter, call: ChatRoomCallModel?): Result<Unit> =
            extensionContext.setRoomCall(chatIdParameter.toChatId(id), call)

        override suspend fun setRoomContact(chatIdParameter: ProductChatIdParameter, contact: ChatRoomContactModel): Result<Unit> =
            extensionContext.setRoomContact(chatIdParameter.toChatId(id), contact)

        // An active call stays: the bar of a call follows the call, not the worker.
        override suspend fun clearRingingCalls(): Result<Unit> = runCancellableCatching {
            extensionContext.subscribeOwnRooms().first()
                .filter { callRepository.observe(it).first() is ChatRoomCallModel.Ringing }
                .forEach { extensionContext.setRoomCall(it, null).getOrThrow() }
        }

        private fun ProductBotMessage.toChatMessageContent(): ChatMessage.Content = when (this) {
            is ProductBotMessage.Text -> ChatMessage.Content.Text(text)
            is ProductBotMessage.Custom -> {
                val messageContent = ProductsMessageContent(messageType, data, alt)
                ChatMessage.Content.Custom(
                    rendererId = messageRenderer.id,
                    content = Result.success(messageContent)
                )
            }
        }
    }
}
