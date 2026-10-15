package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import kotlinx.coroutines.flow.Flow

/**
 * Where the room screen reads its messages and where it writes what the user does (02-DESIGN §8.3, §9).
 * [HostStoreMessageSource] reads the host database. [ProductMessageSource] asks the product for pages and
 * sends the actions of the user to the product.
 */
interface ChatMessageSource {
    /** The window of messages. A new value comes each time the window changes. */
    fun observeSnapshots(): Flow<ChatFeedSnapshot>

    /** Asks for the page before the oldest message of the window. */
    suspend fun loadOlder(): ChatPageOutcome

    /** Asks for the page after the newest message of the window. */
    suspend fun loadNewer(): ChatPageOutcome

    /** Asks for pages until the message is in the window. False when both ends of the room come first. */
    suspend fun jump(messageId: ChatMessageId): Boolean

    suspend fun perform(action: ChatSourceAction): ChatSourceActionOutcome

    /** Asks for the newest page again, for example after the room became unavailable. */
    suspend fun reload()

    /** Shows "unavailable" and keeps the window, for example when the worker of the product cannot start. */
    suspend fun markUnavailable()

    /** Stops the work of the source. The source gives no more snapshots. */
    fun close()
}

/** The window of messages that the room screen shows. */
data class ChatFeedSnapshot(
    /** The newest message first, as the host database gives them. */
    val messages: List<ChatMessage>,
    val reactions: Map<ChatMessageId, List<ChatMessageReaction>>,
    /** The latest body of each edited message. */
    val revisions: Map<ChatMessageId, MessageRevision>,
    /** The status that the product gave to each message of the user. */
    val productStatuses: Map<ChatMessageId, ProductMessageStatusModel>,
    val paging: ChatPagingState,
    val readiness: ChatRoomReadiness,
    /** The messages of the product whose status is `Preparing`, for the reasoning rows (07-LATER §9.4). */
    val preparingProductMessages: Set<ChatMessageId> = emptySet(),
)

/** What the room screen can ask at each end of the window. */
data class ChatPagingState(val older: Edge, val newer: Edge) {
    enum class Edge {
        /** More messages exist past this end. */
        MORE,

        /** A page for this end is on its way. */
        LOADING,

        /** No more messages exist past this end. */
        END,

        /** The last page for this end did not arrive. The room screen can ask again. */
        FAILED,
    }

    companion object {
        /** A window that holds the whole room, as the host database gives it. */
        val Complete = ChatPagingState(older = Edge.END, newer = Edge.END)
    }
}

/** Whether the room can answer the room screen. */
enum class ChatRoomReadiness {
    /** The first page is on its way. */
    LOADING,
    READY,

    /** The product does not answer. The window keeps the cached pages. */
    UNAVAILABLE,

    /** The product did not give the newest page in time, or refused it (02-DESIGN §3.6). The window keeps the cached pages. */
    FAILED,
}

sealed interface ChatPageOutcome {
    /** The page added this number of messages to the window. */
    data class Appended(val count: Int) : ChatPageOutcome

    /** No more messages exist past this end. */
    data object End : ChatPageOutcome

    /** The page did not arrive in time, or the product refused to give it. */
    data object Failed : ChatPageOutcome

    /** The product is not available. */
    data object Unavailable : ChatPageOutcome
}

/** An action of the user on the messages of a room. Ids are the ids of the window. */
sealed interface ChatSourceAction {
    data class Send(val content: ChatMessage.Content, val replyTo: ChatMessageId?) : ChatSourceAction

    data class Edit(val messageId: ChatMessageId, val content: ChatMessage.Content) : ChatSourceAction

    data class Delete(val messageId: ChatMessageId) : ChatSourceAction

    /** Adds the reaction of the user, or removes it when the user has it. */
    data class ToggleReaction(val messageId: ChatMessageId, val emoji: String) : ChatSourceAction

    /** The user saw these messages. */
    data class MarkSeen(val messageIds: Set<ChatMessageId>) : ChatSourceAction

    /** Sends a failed message again. */
    data class Retry(val messageId: ChatMessageId) : ChatSourceAction
}

sealed interface ChatSourceActionOutcome {
    data object Done : ChatSourceActionOutcome

    /** The product refused the action, and the window shows the state before it. [reason] is for the user. */
    data class Refused(val reason: String?) : ChatSourceActionOutcome

    /** The message stays in the window with the failed status. The user can retry it. */
    data object Failed : ChatSourceActionOutcome
}
