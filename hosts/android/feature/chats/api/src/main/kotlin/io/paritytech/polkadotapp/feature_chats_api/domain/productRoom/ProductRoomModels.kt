package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage

/** The reserved participant ids of Chat Extension v2. */
object ProductParticipant {
    /** The user of the host. */
    const val USER = "user"

    /** The product itself. */
    const val PRODUCT = "product"

    /** The start of the participant id `bot:<botId>` of a bot persona of the product (07-LATER §5.2). */
    const val PERSONA_PREFIX = "bot:"

    /** The bot id of a persona participant id, or null for every other participant id. */
    fun personaBotId(participantId: String): String? =
        participantId.takeIf { it.startsWith(PERSONA_PREFIX) }?.removePrefix(PERSONA_PREFIX)?.takeIf { it.isNotEmpty() }
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
)

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
    data class NewMessage(val content: ChatMessage.Content, val replyTo: String?) : ChatUserActionModel

    data class EditMessage(val messageId: String, val content: ChatMessage.Content) : ChatUserActionModel

    data class DeleteMessage(val messageId: String) : ChatUserActionModel

    data class ReactionPlaced(val messageId: String, val emoji: String) : ChatUserActionModel

    data class ReactionRemoved(val messageId: String, val emoji: String) : ChatUserActionModel

    data class Seen(val messageId: String) : ChatUserActionModel

    data class Retry(val messageId: String) : ChatUserActionModel
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

    data class Deleted(val messageId: String) : ProductFeedUpdateModel

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

/** Something the user did in a product room that goes to the product as an event. Ids are host ids. */
sealed interface ChatRoomUserEvent {
    data class MessageDeleted(val messageId: String) : ChatRoomUserEvent

    /** The newest message that the user saw. */
    data class MessageSeen(val messageId: String) : ChatRoomUserEvent

    /** A press on a button of an Actions message. */
    data class ActionTriggered(val messageId: String, val actionId: String) : ChatRoomUserEvent

    data class MessageActionTriggered(val messageId: String, val actionId: String) : ChatRoomUserEvent

    data class ParticipantActionTriggered(val participantId: String, val actionId: String) : ChatRoomUserEvent

    data class ToolbarActionTriggered(val actionId: String) : ChatRoomUserEvent

    data class SuggestionSelected(val suggestionId: String) : ChatRoomUserEvent
}
