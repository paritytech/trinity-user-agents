package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage

internal val ProductRoomMessage.isFromUser: Boolean
    get() = participantId == ProductParticipant.USER

/** The welcome text of a chat request draws as a request message, also after an edit. */
private val ProductRoomMessage.displayContent: ChatMessage.Content
    get() = when (val content = content) {
        is ChatMessage.Content.Text -> if (request) ChatMessage.Content.ChatRequest(ChatMessage.Content.RichText(content.text, emptyList())) else content
        else -> content
    }

/** The status of a message of the product tells the room screen whether the user saw it. */
internal fun ProductRoomMessage.toChatMessage(chatId: ChatId, extensionId: ChatExtensionId, isUnread: Boolean) = ChatMessage(
    id = messageId,
    chatId = chatId,
    timestamp = timestamp,
    origin = if (isFromUser) ChatMessageOrigin.User else ChatMessageOrigin.Extension(extensionId),
    content = displayContent,
    status = when {
        isFromUser -> status.toChatStatus()
        isUnread -> ChatMessage.Status.NEW
        else -> ChatMessage.Status.IS_READ
    },
    replyToMessageId = replyTo,
)

private fun ProductMessageStatusModel.toChatStatus(): ChatMessage.Status = when (this) {
    ProductMessageStatusModel.PREPARING -> ChatMessage.Status.NEW
    ProductMessageStatusModel.SENT -> ChatMessage.Status.IS_SENT
    // A person chat has no separate "delivered": the double tick is a device of the contact that has it.
    ProductMessageStatusModel.DELIVERED,
    ProductMessageStatusModel.READ -> ChatMessage.Status.IS_READ
    ProductMessageStatusModel.FAILED -> ChatMessage.Status.DELIVERY_FAILED
}
