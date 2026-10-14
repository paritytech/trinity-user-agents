package io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.bot.sample

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBot
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotContext
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotData
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_impl.presentation.bot.EchoContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.bot.EchoMessageRenderer
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SampleBot @Inject constructor(
    override val customChatPreviewDelegate: SampleBotChatPreviewDelegate
) : ChatBot() {
    private val data = ChatBotData.sample()

    override val id: ChatExtensionId = data.id
    override val metadata = ChatBotMetadata(data.name, icon = null)

    override fun customMessageRenderers(): List<CustomChatMessageRenderer<*>> {
        return listOf(EchoMessageRenderer)
    }

    context(chatBotContext: ChatBotContext)
    override fun startBotWork() {
        chatBotContext.scope.launch {
            chatBotContext.setWelcomeMessages { listOf(ChatMessage.Content.Text("Hello, want to chat? I'll echo your messages with a custom style!")) }
        }
    }

    context(chatBotContext: ChatBotContext)
    override fun onTextMessage(
        message: ChatMessage,
        content: ChatMessage.Content.Text
    ) {
        chatBotContext.scope.launch {
            val customContent = createEchoContent(content.text)
            chatBotContext.sendMessage(customContent)
        }
    }

    private fun createEchoContent(originalText: String): ChatMessage.Content.Custom<EchoContent> {
        val content = EchoContent(originalText, watchTime = 0)
        return ChatMessage.Content.Custom(EchoMessageRenderer.id, Result.success(content))
    }
}
