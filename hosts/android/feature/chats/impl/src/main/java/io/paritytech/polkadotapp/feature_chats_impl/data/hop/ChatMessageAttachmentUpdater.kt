package io.paritytech.polkadotapp.feature_chats_impl.data.hop

import android.net.Uri
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Attachment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import javax.inject.Inject

class ChatMessageAttachmentUpdater @Inject constructor(
    private val chatEngine: ChatEngine
) {
    suspend fun updateSentAttachment(
        chatId: ChatId,
        messageId: ChatMessageId,
        attachment: Attachment.Hosted
    ) {
        val message = chatEngine.awaitMessage(messageId)
        val existingContent = message.content as? ChatMessage.Content.RichText

        val updatedContent = ChatMessage.Content.RichText(
            text = existingContent?.text,
            attachments = listOf(attachment)
        )

        chatEngine.updateMessageContent(chatId, messageId, updatedContent)
        chatEngine.updateMessageStatus(messageId, ChatMessage.Status.NEW)
    }

    suspend fun updateReceivedAttachment(
        chatId: ChatId,
        messageId: ChatMessageId,
        identifier: DataByteArray,
        uri: Uri
    ) {
        val message = chatEngine.awaitMessage(messageId)
        val existingContent = message.content as? ChatMessage.Content.RichText ?: return

        val updatedAttachments = existingContent.attachments.map { existing ->
            if (existing is Attachment.Hosted && existing.identifier == identifier) {
                existing.copy(uri = uri)
            } else {
                existing
            }
        }

        val updatedContent = ChatMessage.Content.RichText(
            text = existingContent.text,
            attachments = updatedAttachments
        )

        chatEngine.updateMessageContent(chatId, messageId, updatedContent)
    }
}
