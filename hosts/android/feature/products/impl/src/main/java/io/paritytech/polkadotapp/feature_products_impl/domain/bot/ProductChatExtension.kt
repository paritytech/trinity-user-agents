package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.DefaultRoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.NewMessagesRoomFilter
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageRenderer
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.extractProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.toChatId
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.WorkerModalityApi
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu.ProductChatMenuRenderer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
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
 */
class ProductChatExtension(
    appContext: Context,
    val product: Product,
    private val workerRefCounter: ProductWorkerRefCounter,
) : ExternalExtension() {
    override val id = product.id.toChatExtensionId()

    override val activationStateExternallyControlled: Boolean = false

    override val defaultRoomMetadata: DefaultRoomMetadata = DefaultRoomMetadata(product.name, icon = null)

    private val runningWorker = DeferredProductWorker()
    private val messageRenderer = ProductsMessageRenderer(appContext, product, runningWorker)

    private var botScope: CoroutineScope? = null

    // Strong ref to the messaging target bound weakly onto the shared worker; held for our lifetime.
    private var chatMessaging: ProductChatMessaging? = null

    override fun customMessageRenderers(): List<CustomChatMessageRenderer<*>> {
        return listOf(messageRenderer)
    }

    override fun customMenuRenderer(chatId: ChatId): CustomChatMenuRenderer = ProductChatMenuRenderer(product, chatId)

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
                .onEach { message -> routeMessage(message) }
                .launchIn(chatExtensionContext.scope)

            // Enable chat messaging before the worker's started hook so an initial/welcome message
            // routes. The reference is released from the finally so a dispose that races boot never
            // leaks the acquisition.
            val reference = workerRefCounter.acquire(product.id, "chat:${product.id.value}")
            try {
                reference.enableModalityApi(WorkerModalityApi.Chat(messaging))
                runningWorker.attach(reference.worker())
                awaitCancellation()
            } finally {
                withContext(NonCancellable) { reference.release() }
                runningWorker.attach(null)
            }
        }
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other !is ProductChatExtension) return false
        return product == other.product
    }

    override fun hashCode(): Int = product.hashCode()

    override suspend fun dispose() {
        // Cancelling the scope runs the finally in startGlobalWork, which releases the worker lease.
        botScope?.cancel()
        botScope = null
        chatMessaging = null
        Timber.d("Disposed product bot: ${product.name}")
    }

    private suspend fun routeMessage(message: ChatMessage) {
        when (val content = message.content) {
            is ChatMessage.Content.Text -> runningWorker.onUserMessage(content.text)
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

        override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> {
            return extensionContext.subscribeOwnRooms().map { chatIds ->
                chatIds.mapNotNull { chatId ->
                    val param = chatId.extractProductChatIdParameter(id)
                        .logFailure("Unexpected state: subscribeOwnRooms returned corrupted chatId: ${chatId.value}")
                        .getOrNull() ?: return@mapNotNull null

                    ProductChatRoom(roomId = param.value, participatingAs = "RoomHost")
                }
            }
        }

        private fun ProductBotMessage.toChatMessageContent(): ChatMessage.Content = when (this) {
            is ProductBotMessage.Text -> ChatMessage.Content.Text(text)
            is ProductBotMessage.Custom -> {
                val messageContent = ProductsMessageContent(messageType, data)
                ChatMessage.Content.Custom(
                    rendererId = messageRenderer.id,
                    content = Result.success(messageContent)
                )
            }
        }
    }
}
