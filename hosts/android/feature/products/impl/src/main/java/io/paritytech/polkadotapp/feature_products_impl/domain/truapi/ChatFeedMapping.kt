package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.ChatQueryException
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
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReplyPreviewModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
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

/** The body that a product receives for the content of a message, or null for a content that no body carries. */
fun ChatMessage.Content.toCoreBody(): ChatMessageBody? = when (this) {
    is ChatMessage.Content.Text -> ChatMessageBody.Text(text)
    // The attachments of a v1 rich text are network uploads that no product fetches, so only the text goes.
    is ChatMessage.Content.RichText -> text?.let { ChatMessageBody.Text(it) }
    is ChatMessage.Content.ProductBody -> when (val body = body) {
        is ProductMessageBody.RichText -> ChatMessageBody.RichText(body.text, body.media.map { it.toCore() })
    }
    else -> null
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

fun ChatPageAnchorModel.toCore(): ChatPageAnchor = when (this) {
    is ChatPageAnchorModel.After -> ChatPageAnchor.After(cursor)
    is ChatPageAnchorModel.Before -> ChatPageAnchor.Before(cursor)
}

/** Null when a new or edited message has no body for the product. */
fun ChatUserActionModel.toCore(): ChatUserAction? = when (this) {
    is ChatUserActionModel.NewMessage -> content.toCoreBody()?.let { ChatUserAction.NewMessage(it, replyTo, messageId, timestamp?.toULong()) }
    is ChatUserActionModel.EditMessage -> content.toCoreBody()?.let { ChatUserAction.EditMessage(messageId, it) }
    is ChatUserActionModel.ReactionPlaced -> ChatUserAction.ReactionPlaced(messageId, emoji)
    is ChatUserActionModel.ReactionRemoved -> ChatUserAction.ReactionRemoved(messageId, emoji)
    is ChatUserActionModel.Seen -> ChatUserAction.Seen(messageId)
}

fun ProductChatMessagesQueryItem.toModel() =
    ProductMessagesPage(messages = messages.map { it.toModel() }, next = next)

fun ProductChatUserActionItem.toModel(): ChatUserActionOutcome = when (this) {
    is ProductChatUserActionItem.MessageCreated -> ChatUserActionOutcome.MessageCreated(v1.toModel())
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
