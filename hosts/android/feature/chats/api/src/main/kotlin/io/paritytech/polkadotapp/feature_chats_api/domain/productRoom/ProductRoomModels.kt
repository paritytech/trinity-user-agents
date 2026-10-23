package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage

/** The reserved participant ids of Chat Extension v2. */
object ProductParticipant {
    /** The user of the host. */
    const val USER = "user"

    /** The product itself. */
    const val PRODUCT = "product"

    /** The contact of a room of the chat product. */
    const val CONTACT = "contact"
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

/** One page of messages, the oldest first. A null [next] ends the paging in this direction. */
data class ProductMessagesPage(val messages: List<ProductRoomMessage>, val next: String?)

sealed interface ChatPageAnchorModel {
    /** Newer messages. A null cursor starts at the oldest. */
    data class After(val cursor: String?) : ChatPageAnchorModel

    /** Older messages. A null cursor starts at the newest. */
    data class Before(val cursor: String?) : ChatPageAnchorModel
}

/** What the user did in a room whose messages the product keeps (`ChatUserAction`). Ids are product ids. */
sealed interface ChatUserActionModel {
    /**
     * [messageId] is the id that the host chose for the message, a UUID as the native chat names its messages. The
     * product sends the message under it, and a second `NewMessage` with the same id sends nothing new. [timestamp] is
     * when the user wrote it, in milliseconds since the Unix epoch: the product sends the message with that time, as the
     * native chat keeps the compose time.
     */
    data class NewMessage(
        val content: ChatMessage.Content,
        val replyTo: String?,
        val messageId: String? = null,
        val timestamp: Long? = null,
    ) : ChatUserActionModel

    data class EditMessage(val messageId: String, val content: ChatMessage.Content) : ChatUserActionModel

    data class ReactionPlaced(val messageId: String, val emoji: String) : ChatUserActionModel

    data class ReactionRemoved(val messageId: String, val emoji: String) : ChatUserActionModel

    data class Seen(val messageId: String) : ChatUserActionModel

    // The answers of the user to the contact of a room of the chat product.

    data object AcceptRequest : ChatUserActionModel

    data object DeclineRequest : ChatUserActionModel

    data object Block : ChatUserActionModel

    data object Unblock : ChatUserActionModel

    data object Leave : ChatUserActionModel

    /**
     * A coinage payment of the user to the contact of a room of the chat product. The host chose [messageId] and
     * committed the handoff of the coins under it; the product sends the native payment message under that id and
     * [timestamp], and a second `SendPayment` with the same id sends nothing new.
     */
    class SendPayment(
        val messageId: String,
        val timestamp: Long,
        val totalValue: Balance,
        val coinKeys: List<ByteArray>,
    ) : ChatUserActionModel
}

sealed interface ChatUserActionOutcome {
    /** The answer to [ChatUserActionModel.NewMessage]: the message as the product keeps it. */
    data class MessageCreated(val message: ProductRoomMessage) : ChatUserActionOutcome

    data object Acknowledged : ChatUserActionOutcome
}

/** Why a query to the product has no answer. */
sealed class ProductChatQueryException(message: String, cause: Throwable? = null) : Exception(message, cause) {
    /** The product cannot answer this question. The host hides the feature; this is not a failure. */
    class NotAvailable : ProductChatQueryException("the product does not answer this question")

    class UnknownRoom : ProductChatQueryException("the product does not know the room")

    class UnknownMessage : ProductChatQueryException("the product does not know the message")

    /** The product refused the action of the user. Show [reason] to the user. */
    class Rejected(val reason: String) : ProductChatQueryException(reason)

    class Failed(val reason: String) : ProductChatQueryException(reason)

    class NoAnswer : ProductChatQueryException("the product ended the query without an answer")

    class TimedOut : ProductChatQueryException("the product did not answer in time")

    /** The worker of the product does not run, or the host does not serve the query. */
    class Unavailable(cause: Throwable?) : ProductChatQueryException("the product is not available", cause)
}

/** One change that a product pushes to the messages of a room (`ChatFeedUpdate`). Ids are product ids. */
sealed interface ProductFeedUpdateModel {
    data class New(val message: ProductRoomMessage) : ProductFeedUpdateModel

    data class Edited(val messageId: String, val content: ChatMessage.Content, val editedAt: Long) : ProductFeedUpdateModel

    /** The message changed for a reason that is not an edit, such as its call ending. It is not marked edited. */
    data class Replaced(val message: ProductRoomMessage) : ProductFeedUpdateModel

    data class ReactionPlaced(val messageId: String, val participantId: String, val emoji: String) : ProductFeedUpdateModel

    data class ReactionRemoved(val messageId: String, val participantId: String, val emoji: String) : ProductFeedUpdateModel

    data class StatusChanged(val messageId: String, val status: ProductMessageStatusModel) : ProductFeedUpdateModel
}

/** The room does not exist, or another extension owns it. */
class UnknownChatRoomException : Exception("the chat room does not exist")

/** Why the host refuses a feed update for a room that it stores (`NativeChatFeedRejection`). */
sealed class ProductFeedRejection(message: String) : Exception(message) {
    class UnknownRoom : ProductFeedRejection("the room does not exist")

    class UnknownMessage : ProductFeedRejection("the room has no such message")

    /** A `New` whose id the room has already. */
    class DuplicateMessage : ProductFeedRejection("the room has a message with this id already")

    /** The product tried to write as the user, or to edit a message that it did not write. */
    class NotOwner : ProductFeedRejection("the product does not own this message")
}
