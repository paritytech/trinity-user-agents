package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotStateController
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOpenHandler
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOverlayRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewDelegate
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatConfig
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatAppearance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf

interface ChatExtension {
    val id: ChatExtensionId

    val defaultRoomMetadata: DefaultRoomMetadata?
        get() = null

    /**
     * Whether activation state is controlled by [ChatBotStateController]
     */
    val activationStateExternallyControlled: Boolean

    context(chatExtensionContext: ChatExtensionContext)
    fun startGlobalWork()

    fun customFooterRenderer(chatId: ChatId): CustomChatFooterRenderer? = null
    fun customMenuRenderer(chatId: ChatId): CustomChatMenuRenderer? = null
    fun customChatAppearance(chatId: ChatId): CustomChatAppearance? = null

    /**
     * Optional takeover of opening this chat from the chat list. When this returns a handler and
     * [CustomChatOpenHandler.handleOpen] returns true, the host skips opening the default chat feed.
     */
    fun customOpenHandler(chatId: ChatId): CustomChatOpenHandler? = null

    /** Optional global overlay rendered above the app shell whenever this extension is active. */
    fun customGlobalOverlayRenderer(): CustomChatOverlayRenderer? = null

    /** FQNs of fragments owned by this extension. The shell suppresses [customGlobalOverlayRenderer] on these. */
    fun ownedFragmentClasses(): Set<String> = emptySet()

    // TODO could be more precise, e.g. per chat
    fun customMessageRenderers(): List<CustomChatMessageRenderer<*>> = emptyList()
    fun customChatPreviewDelegate(chatId: ChatId): CustomChatPreviewDelegate<*>? = null

    /**
     * Whether this extension's messages should be revealed one-by-one with a typing animation on
     * first appearance. Pure presentation: messages are always persisted instantly; the chat feed
     * gates how many are shown and animates the newest unrevealed one. Defaults to no animation.
     */
    fun animatesMessageReveal(chatId: ChatId): Boolean = false

    fun observeUserInputAllowed(chatId: ChatId): Flow<Boolean> = flowOf(true)
    fun observeChatConfig(chatId: ChatId): Flow<ChatConfig> = flowOf(ChatConfig.Default)
}
