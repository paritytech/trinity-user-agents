package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReplyPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatMessage as CoreChatMessage

// Maps the messages that cross the wire to the chat models of the app, and back.

// The events, the calls and the payments belong to the Chat Extension v2 additions, which this host does not serve.
fun ChatMessageBody.toContent(): ChatMessage.Content = when (this) {
    is ChatMessageBody.Text -> ChatMessage.Content.Text(text)
    is ChatMessageBody.RichText -> if (media.isEmpty()) {
        ChatMessage.Content.Text(text.orEmpty())
    } else {
        ChatMessage.Content.ProductBody(ProductMessageBody.RichText(text, media.map { it.toModel() }))
    }
    is ChatMessageBody.Event,
    is ChatMessageBody.Call,
    is ChatMessageBody.Payment,
    -> ChatMessage.Content.Unsupported(ByteArray(0))
}

/** The welcome text of a chat request draws as a request message. */
private fun ChatMessageBody.toMessageContent(request: Boolean): ChatMessage.Content {
    val content = toContent()
    if (!request) return content
    return when (content) {
        is ChatMessage.Content.Text -> ChatMessage.Content.ChatRequest(ChatMessage.Content.RichText(content.text, emptyList()))
        else -> content
    }
}

fun CoreChatMessage.toModel() = ProductRoomMessage(
    messageId = messageId,
    participantId = origin.participantId,
    timestamp = timestamp.toLong(),
    content = body.toMessageContent(request),
    status = status.toModel(),
    replyTo = replyTo,
    editedAt = editedAt?.toLong(),
    reactions = reactions.map { ProductReactionSummaryModel(it.emoji, it.count.toInt(), it.reactedByUser) },
    request = request,
    replyPreview = replyPreview?.let { ProductReplyPreviewModel(it.origin.participantId, it.body.toContent()) },
)

fun ChatFeedUpdate.toModel(): ProductFeedUpdateModel = when (this) {
    is ChatFeedUpdate.New -> ProductFeedUpdateModel.New(v1.toModel())
    is ChatFeedUpdate.Edited -> ProductFeedUpdateModel.Edited(messageId, body.toContent(), editedAt.toLong())
    is ChatFeedUpdate.ReactionPlaced -> ProductFeedUpdateModel.ReactionPlaced(messageId, participantId, emoji)
    is ChatFeedUpdate.ReactionRemoved -> ProductFeedUpdateModel.ReactionRemoved(messageId, participantId, emoji)
    is ChatFeedUpdate.StatusChanged -> ProductFeedUpdateModel.StatusChanged(messageId, status.toModel())
    is ChatFeedUpdate.Replaced -> ProductFeedUpdateModel.Replaced(v1.toModel())
}

fun ChatMessageStatus.toModel(): ProductMessageStatusModel = when (this) {
    ChatMessageStatus.PREPARING -> ProductMessageStatusModel.PREPARING
    ChatMessageStatus.SENT -> ProductMessageStatusModel.SENT
    ChatMessageStatus.DELIVERED -> ProductMessageStatusModel.DELIVERED
    ChatMessageStatus.READ -> ProductMessageStatusModel.READ
    ChatMessageStatus.FAILED -> ProductMessageStatusModel.FAILED
}
