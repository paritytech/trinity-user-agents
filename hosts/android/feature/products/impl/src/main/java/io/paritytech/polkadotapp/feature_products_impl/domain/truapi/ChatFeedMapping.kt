package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ChatQueryException
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatPageAnchor
import uniffi.truapi.ChatUserAction
import uniffi.truapi.ProductChatMessagesQueryItem
import uniffi.truapi.ProductChatQueryError
import uniffi.truapi.ProductChatUserActionError
import uniffi.truapi.ProductChatUserActionItem
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

fun ChatPageAnchorModel.toCore(): ChatPageAnchor = when (this) {
    is ChatPageAnchorModel.After -> ChatPageAnchor.After(cursor)
    is ChatPageAnchorModel.Before -> ChatPageAnchor.Before(cursor)
}

/** Null when a new or edited message has no body for the product. */
fun ChatUserActionModel.toCore(): ChatUserAction? = when (this) {
    is ChatUserActionModel.NewMessage -> content.toCoreBody()?.let { ChatUserAction.NewMessage(it, replyTo) }
    is ChatUserActionModel.EditMessage -> content.toCoreBody()?.let { ChatUserAction.EditMessage(messageId, it) }
    is ChatUserActionModel.DeleteMessage -> ChatUserAction.DeleteMessage(messageId)
    is ChatUserActionModel.ReactionPlaced -> ChatUserAction.ReactionPlaced(messageId, emoji)
    is ChatUserActionModel.ReactionRemoved -> ChatUserAction.ReactionRemoved(messageId, emoji)
    is ChatUserActionModel.Seen -> ChatUserAction.Seen(messageId)
    is ChatUserActionModel.Retry -> ChatUserAction.Retry(messageId)
}

fun ProductChatMessagesQueryItem.toModel(rendererId: String) =
    ProductMessagesPage(messages = messages.map { it.toModel(rendererId) }, next = next)

fun ProductChatUserActionItem.toModel(rendererId: String): ChatUserActionOutcome = when (this) {
    is ProductChatUserActionItem.MessageCreated -> ChatUserActionOutcome.MessageCreated(v1.toModel(rendererId))
    ProductChatUserActionItem.Acknowledged -> ChatUserActionOutcome.Acknowledged
}

/** The failure that a room screen shows for a query that has no answer. */
fun Throwable.toQueryException(): ProductChatQueryException = when (this) {
    is ProductChatQueryException -> this
    is ChatQueryException.Refused -> when (val error = error) {
        ProductChatQueryError.NotAvailable -> ProductChatQueryException.NotAvailable()
        ProductChatQueryError.UnknownRoom -> ProductChatQueryException.UnknownRoom()
        is ProductChatQueryError.Unknown -> ProductChatQueryException.Failed(error.reason)
    }
    is ChatQueryException.ActionRefused -> when (val error = error) {
        ProductChatUserActionError.UnknownRoom -> ProductChatQueryException.UnknownRoom()
        ProductChatUserActionError.UnknownMessage -> ProductChatQueryException.UnknownMessage()
        is ProductChatUserActionError.Rejected -> ProductChatQueryException.Rejected(error.reason)
        is ProductChatUserActionError.Unknown -> ProductChatQueryException.Failed(error.reason)
    }
    is ChatQueryException.Failed -> ProductChatQueryException.Failed(reason)
    is ChatQueryException.NoAnswer -> ProductChatQueryException.NoAnswer()
    is ChatQueryException.TimedOut -> ProductChatQueryException.TimedOut()
    else -> ProductChatQueryException.Unavailable(this)
}
