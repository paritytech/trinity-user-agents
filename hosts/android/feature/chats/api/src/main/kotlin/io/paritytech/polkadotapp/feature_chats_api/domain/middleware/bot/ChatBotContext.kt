package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

class ChatBotContext(
    private val extensionContext: ChatExtensionContext,
    val chatId: ChatId,
) : ComputationalScope by extensionContext.scope {
    val scope: ComputationalScope get() = this

    suspend fun sendMessage(content: ChatMessage.Content): ChatMessage =
        extensionContext.sendMessage(chatId, content)

    suspend fun modifyMessage(messageId: ChatMessageId, content: ChatMessage.Content) =
        extensionContext.modifyMessage(chatId, messageId, content)

    suspend fun removeMessage(messageId: ChatMessageId) =
        extensionContext.removeMessage(chatId, messageId)

    suspend fun setWelcomeMessages(messageBuilder: () -> List<ChatMessage.Content>) =
        extensionContext.setWelcomeMessages(chatId, messageBuilder)

    suspend fun getPersistedMessages(): List<ChatMessage> =
        extensionContext.getPersistedMessages(chatId)

    suspend fun <T> sendCustomMessage(
        content: T,
        rendererId: CustomChatMessageRendererId,
    ): ChatMessage {
        val messageContent = ChatMessage.Content.Custom(rendererId, Result.success(content))
        return sendMessage(messageContent)
    }

    suspend fun upsertMessage(
        existingMessageId: ChatMessageId?,
        content: ChatMessage.Content,
    ): ChatMessageId {
        return if (existingMessageId != null) {
            modifyMessage(existingMessageId, content)
            existingMessageId
        } else {
            sendMessage(content).id
        }
    }
}
