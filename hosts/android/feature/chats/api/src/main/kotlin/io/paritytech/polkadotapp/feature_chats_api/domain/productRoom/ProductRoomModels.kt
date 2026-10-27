package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage

/** The reserved participant ids of Chat Extension v2. */
object ProductParticipant {
    /** The user of the host. */
    const val USER = "user"

    /** The product itself. */
    const val PRODUCT = "product"
}

/** The status of a message as its product gives it (`ChatMessageStatus`). */
enum class ProductMessageStatusModel { PREPARING, SENT, DELIVERED, READ, FAILED }

data class ProductReactionSummaryModel(val emoji: String, val count: Int, val reactedByUser: Boolean)

/** A message of a product room as the product gives it (`ChatMessage`). */
data class ProductRoomMessage(
    /** The id that the product picked. It is unique in the room only. */
    val messageId: String,
    val participantId: String,
    /** Milliseconds since the Unix epoch. */
    val timestamp: Long,
    val content: ChatMessage.Content,
    val status: ProductMessageStatusModel,
    /** The id of the message that this message replies to, as the product names it. */
    val replyTo: String?,
    val editedAt: Long?,
    val reactions: List<ProductReactionSummaryModel>,
    /** The message is the welcome message of a chat request. */
    val request: Boolean,
    /** The message that [replyTo] names, so the quote shows while that message is not loaded. */
    val replyPreview: ProductReplyPreviewModel? = null,
)

/** The author and the body of a replied message (`ChatReplyPreview`). */
data class ProductReplyPreviewModel(val participantId: String, val content: ChatMessage.Content)

/** The room does not exist, or another extension owns it. */
class UnknownChatRoomException : Exception("the chat room does not exist")

/** One change that a product pushes to the messages of a room (`ChatFeedUpdate`). Ids are product ids. */
sealed interface ProductFeedUpdateModel {
    data class New(val message: ProductRoomMessage) : ProductFeedUpdateModel

    data class Edited(val messageId: String, val content: ChatMessage.Content, val editedAt: Long) : ProductFeedUpdateModel

    /** The message changed for a reason that is not an edit. It is not marked edited. */
    data class Replaced(val message: ProductRoomMessage) : ProductFeedUpdateModel

    data class ReactionPlaced(val messageId: String, val participantId: String, val emoji: String) : ProductFeedUpdateModel

    data class ReactionRemoved(val messageId: String, val participantId: String, val emoji: String) : ProductFeedUpdateModel

    data class StatusChanged(val messageId: String, val status: ProductMessageStatusModel) : ProductFeedUpdateModel
}

/** Why the host refuses a feed update for a room that it stores (`NativeChatFeedRejection`). */
sealed class ProductFeedRejection(message: String) : Exception(message) {
    class UnknownRoom : ProductFeedRejection("the room does not exist")

    class UnknownMessage : ProductFeedRejection("the room has no such message")

    /** A `New` whose id the room has already. */
    class DuplicateMessage : ProductFeedRejection("the room has a message with this id already")

    /** The product tried to write as the user, or to edit a message that it did not write. */
    class NotOwner : ProductFeedRejection("the product does not own this message")
}
