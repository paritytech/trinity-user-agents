package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.DefaultRoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.NewMessagesRoomFilter
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

abstract class ChatBot : ChatExtension {
    abstract val metadata: ChatBotMetadata

    val chatId: ChatId get() = ChatId.forExtension(id)

    // --- ChatExtension implementation (final) ---

    final override val defaultRoomMetadata: DefaultRoomMetadata
        get() = DefaultRoomMetadata(metadata.name, icon = metadata.icon?.toString())

    context(chatExtensionContext: ChatExtensionContext)
    final override fun startGlobalWork() {
        chatExtensionContext.scope.launch { chatExtensionContext.createRoom(CreateRoomRequest(chatId = chatId, name = null, icon = null)) }

        val botContext = ChatBotContext(chatExtensionContext, chatId)

        chatExtensionContext.subscribeNewMessages(NewMessagesRoomFilter.Chat(chatId))
            .filter { it.origin !is ChatMessageOrigin.Extension }
            .onEach { message ->
                with(botContext) { routeMessage(message) }
            }
            .launchIn(chatExtensionContext.scope)

        with(botContext) { startBotWork() }
    }

    final override val activationStateExternallyControlled: Boolean = true

    final override fun customFooterRenderer(chatId: ChatId): CustomChatFooterRenderer? = customFooterRenderer
    final override fun customMenuRenderer(chatId: ChatId): CustomChatMenuRenderer? = customMenuRenderer
    final override fun customChatAppearance(chatId: ChatId): CustomChatAppearance? = customChatAppearance
    final override fun customOpenHandler(chatId: ChatId): CustomChatOpenHandler? = customOpenHandler
    final override fun customGlobalOverlayRenderer(): CustomChatOverlayRenderer? = customGlobalOverlayRenderer
    final override fun customChatPreviewDelegate(chatId: ChatId): CustomChatPreviewDelegate<*>? = customChatPreviewDelegate
    final override fun animatesMessageReveal(chatId: ChatId): Boolean = animatesMessageReveal
    final override fun observeUserInputAllowed(chatId: ChatId): Flow<Boolean> = observeUserInputAllowed()
    final override fun observeChatConfig(chatId: ChatId): Flow<ChatConfig> = observeChatConfig()

    // --- Bot-specific API (existing bots override these) ---

    context(chatBotContext: ChatBotContext)
    abstract fun startBotWork()

    open val customFooterRenderer: CustomChatFooterRenderer? = null
    open val customMenuRenderer: CustomChatMenuRenderer? = null
    open val customChatAppearance: CustomChatAppearance? = null
    open val customOpenHandler: CustomChatOpenHandler? = null
    open val customGlobalOverlayRenderer: CustomChatOverlayRenderer? = null
    open val customChatPreviewDelegate: CustomChatPreviewDelegate<*>? = null
    open val animatesMessageReveal: Boolean = false
    open fun observeUserInputAllowed(): Flow<Boolean> = flowOf(true)
    open fun observeChatConfig(): Flow<ChatConfig> = flowOf(ChatConfig.Default)

    context(chatBotContext: ChatBotContext)
    open fun onTextMessage(message: ChatMessage, content: ChatMessage.Content.Text) {}

    open fun onCustomMessage(message: ChatMessage, content: ChatMessage.Content.Custom<*>) {}

    context(chatBotContext: ChatBotContext)
    private fun routeMessage(message: ChatMessage) {
        when (val content = message.content) {
            is ChatMessage.Content.Text -> onTextMessage(message, content)
            is ChatMessage.Content.Custom<*> -> onCustomMessage(message, content)
            else -> {}
        }
    }
}
