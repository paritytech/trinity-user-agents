package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReplyPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatMessage as CoreChatMessage

// Maps the messages that cross the wire to the chat models of the app, and back.

// The host does not draw the media of a body yet, so a rich text keeps its text. The events, the calls and the
// payments belong to the Chat Extension v2 additions, which this host does not serve. The renderer of the product
// draws a custom body, as it draws a custom message that the product posts.
fun ChatMessageBody.toContent(productId: ProductId): ChatMessage.Content = when (this) {
    is ChatMessageBody.Text -> ChatMessage.Content.Text(text)
    is ChatMessageBody.RichText -> ChatMessage.Content.Text(text.orEmpty())
    is ChatMessageBody.Event,
    is ChatMessageBody.Call,
    is ChatMessageBody.Payment,
    -> ChatMessage.Content.Unsupported(ByteArray(0))
    is ChatMessageBody.Custom -> ChatMessage.Content.Custom(
        rendererId = productId.toChatExtensionId(),
        content = Result.success(ProductsMessageContent(message.messageType, message.payload.toDataByteArray(), alt)),
    )
}

/** The welcome text of a chat request draws as a request message. */
private fun ChatMessageBody.toMessageContent(request: Boolean, productId: ProductId): ChatMessage.Content {
    val content = toContent(productId)
    if (!request) return content
    return when (content) {
        is ChatMessage.Content.Text -> ChatMessage.Content.ChatRequest(ChatMessage.Content.RichText(content.text, emptyList()))
        else -> content
    }
}

fun CoreChatMessage.toModel(productId: ProductId) = ProductRoomMessage(
    messageId = messageId,
    participantId = origin.participantId,
    timestamp = timestamp.toLong(),
    content = body.toMessageContent(request, productId),
    status = status.toModel(),
    replyTo = replyTo,
    editedAt = editedAt?.toLong(),
    reactions = reactions.map { ProductReactionSummaryModel(it.emoji, it.count.toInt(), it.reactedByUser) },
    request = request,
    replyPreview = replyPreview?.let { ProductReplyPreviewModel(it.origin.participantId, it.body.toContent(productId)) },
)

fun ChatMessageStatus.toModel(): ProductMessageStatusModel = when (this) {
    ChatMessageStatus.PREPARING -> ProductMessageStatusModel.PREPARING
    ChatMessageStatus.SENT -> ProductMessageStatusModel.SENT
    ChatMessageStatus.DELIVERED -> ProductMessageStatusModel.DELIVERED
    ChatMessageStatus.READ -> ProductMessageStatusModel.READ
    ChatMessageStatus.FAILED -> ProductMessageStatusModel.FAILED
}
