package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatMessage as CoreChatMessage

// Maps the messages that cross the wire to the chat models of the app, and back. [rendererId] is the
// renderer of the product, which draws a custom body.

fun ChatMessageBody.toContent(rendererId: String): ChatMessage.Content = when (this) {
    is ChatMessageBody.Text -> ChatMessage.Content.Text(text)
    is ChatMessageBody.RichText -> ChatMessage.Content.ProductBody(ProductMessageBody.RichText(text, media.map { it.toModel() }))
    is ChatMessageBody.Actions -> ChatMessage.Content.ProductBody(v1.toBody())
    is ChatMessageBody.Custom -> ChatMessage.Content.Custom(
        rendererId = rendererId,
        content = Result.success(ProductsMessageContent(messageType, payload.toDataByteArray(), previewText)),
    )
}

/**
 * The body that a product receives for the content of a message, or null for a content that no body carries.
 * A media item with a v1 address is left out.
 */
fun ChatMessage.Content.toCoreBody(): ChatMessageBody? = when (this) {
    is ChatMessage.Content.Text -> ChatMessageBody.Text(text)
    // A user attachment is not sent to a product yet (P8.6): the text of the message goes alone.
    is ChatMessage.Content.RichText -> text?.let { ChatMessageBody.Text(it) }
    is ChatMessage.Content.ProductBody -> when (val body = body) {
        is ProductMessageBody.RichText -> ChatMessageBody.RichText(body.text, body.media.mapNotNull { it.toCore() })
        is ProductMessageBody.Actions -> ChatMessageBody.Actions(body.toCore())
    }
    is ChatMessage.Content.Custom<*> -> (content.getOrNull() as? ProductsMessageContent)?.let {
        ChatMessageBody.Custom(it.messageType, it.data.value, it.alt)
    }
    else -> null
}

fun CoreChatMessage.toModel(rendererId: String) = ProductRoomMessage(
    messageId = messageId,
    participantId = origin.participantId,
    timestamp = timestamp.toLong(),
    content = body.toContent(rendererId),
    status = status.toModel(),
    replyTo = replyTo,
    editedAt = editedAt?.toLong(),
    reactions = reactions.map { ProductReactionSummaryModel(it.emoji, it.count.toInt(), it.reactedByUser) },
)

fun ChatFeedUpdate.toModel(rendererId: String): ProductFeedUpdateModel = when (this) {
    is ChatFeedUpdate.New -> ProductFeedUpdateModel.New(v1.toModel(rendererId))
    is ChatFeedUpdate.Edited -> ProductFeedUpdateModel.Edited(messageId, body.toContent(rendererId), editedAt.toLong())
    is ChatFeedUpdate.Deleted -> ProductFeedUpdateModel.Deleted(messageId)
    is ChatFeedUpdate.ReactionPlaced -> ProductFeedUpdateModel.ReactionPlaced(messageId, participantId, emoji)
    is ChatFeedUpdate.ReactionRemoved -> ProductFeedUpdateModel.ReactionRemoved(messageId, participantId, emoji)
    is ChatFeedUpdate.StatusChanged -> ProductFeedUpdateModel.StatusChanged(messageId, status.toModel())
}

fun ChatMessageStatus.toModel(): ProductMessageStatusModel = when (this) {
    ChatMessageStatus.PREPARING -> ProductMessageStatusModel.PREPARING
    ChatMessageStatus.SENT -> ProductMessageStatusModel.SENT
    ChatMessageStatus.DELIVERED -> ProductMessageStatusModel.DELIVERED
    ChatMessageStatus.READ -> ProductMessageStatusModel.READ
    ChatMessageStatus.FAILED -> ProductMessageStatusModel.FAILED
}
