package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import kotlinx.coroutines.flow.Flow

/**
 * Where the room screen reads its messages. [HostStoreMessageSource] reads the host database.
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
    val paging: ChatPagingState,
    val readiness: ChatRoomReadiness,
    /** The replied messages that [messages] does not hold, by id, drawn only as the quote of their replies. */
    val replyQuotes: Map<ChatMessageId, ChatMessage> = emptyMap(),
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

    /** The product did not give the newest page in time, or refused it. The window keeps the cached pages. */
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
