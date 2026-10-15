package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.previewText
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidate
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidates
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import javax.inject.Inject

// The search inside one open product room (07-LATER §8.2, D29).

/** One message that matches an in-room search. [messageId] is the id of the room screen. */
data class RoomSearchHit(
    val messageId: ChatMessageId,
    /** Milliseconds since the Unix epoch. */
    val timestamp: Long,
    val text: String,
    val author: RoomSearchAuthor,
)

/** Who wrote a hit. */
sealed interface RoomSearchAuthor {
    /** The user of this device. */
    data object User : RoomSearchAuthor

    /** The other side of the room: the product, or the contact of a person chat. */
    data object Room : RoomSearchAuthor

    /** A participant that the product declares, by its participant id. */
    data class Participant(val participantId: String) : RoomSearchAuthor
}

/** Where the next page of an in-room search starts. */
sealed interface RoomSearchCursor {
    /** After the stored message [messageId] with [timestamp]. */
    data class HostStore(val timestamp: Long, val messageId: ChatMessageId) : RoomSearchCursor

    /** The cursor that the product gave. */
    data class Product(val cursor: String) : RoomSearchCursor
}

sealed interface RoomSearchPage {
    /** Newest first. A null [next] ends the older matches. */
    data class Hits(val hits: List<RoomSearchHit>, val next: RoomSearchCursor?) : RoomSearchPage

    /** The product did not answer, or answered `NotAvailable`: the screen shows "Search is not available". */
    data object NotAvailable : RoomSearchPage
}

/**
 * Whether the header of a room offers the in-room search. The host searches each room that it keeps. A room whose
 * product keeps the messages offers it only when the product declares `search` and the host lists `search`. A
 * person chat keeps its search in the chat list.
 */
fun ChatRoomPolicy?.offersRoomSearch(servedFeatures: Collection<String> = ChatHostCapabilities.features): Boolean {
    if (this == null) return false

    return when (messageStore) {
        ChatRoomPolicy.MessageStore.HOST -> true
        ChatRoomPolicy.MessageStore.PRODUCT -> features.search && ChatHostCapabilities.SEARCH in servedFeatures
    }
}

interface ChatRoomSearchInteractor {
    /**
     * One page of the messages of [chatId] that match [text]. [policy] tells who keeps the messages: the host
     * searches its store, and the product answers `search_query` for a room whose messages it keeps.
     */
    suspend fun search(chatId: ChatId, policy: ChatRoomPolicy, text: String, cursor: RoomSearchCursor?): Result<RoomSearchPage>
}

class RealChatRoomSearchInteractor @Inject constructor(
    private val candidates: ChatRoomSearchCandidates,
    private val chatEngine: ChatEngine,
) : ChatRoomSearchInteractor {
    override suspend fun search(chatId: ChatId, policy: ChatRoomPolicy, text: String, cursor: RoomSearchCursor?): Result<RoomSearchPage> {
        return when (policy.messageStore) {
            ChatRoomPolicy.MessageStore.HOST -> runCancellableCatching {
                searchHostStore(candidates, chatId, foldQuery(text), cursor as? RoomSearchCursor.HostStore)
            }

            ChatRoomPolicy.MessageStore.PRODUCT -> Result.success(searchProduct(chatId, text, cursor as? RoomSearchCursor.Product))
        }
    }

    private suspend fun searchProduct(chatId: ChatId, text: String, cursor: RoomSearchCursor.Product?): RoomSearchPage {
        val querying = chatEngine.roomQuerying(chatId) ?: return RoomSearchPage.NotAvailable

        // A product that does not answer in 10 s, or answers `NotAvailable`, leaves the field without results.
        val page = querying.searchPage(chatId, text.trim(), cursor?.cursor, PRODUCT_SEARCH_PAGE_SIZE).getOrNull()
            ?: return RoomSearchPage.NotAvailable

        return RoomSearchPage.Hits(
            hits = page.hits.take(PRODUCT_SEARCH_PAGE_SIZE).map { it.toHit() },
            next = page.next?.let(RoomSearchCursor::Product),
        )
    }
}

/** The page size that the host asks of a product (07-LATER §3.5). */
internal const val PRODUCT_SEARCH_PAGE_SIZE = 30

/** The page size of the search of the host store (07-LATER §8.2). */
internal const val HOST_SEARCH_PAGE_SIZE = 50

/** How many stored messages the host reads at a time while it fills a page. */
internal const val HOST_SEARCH_CHUNK_SIZE = 200

/**
 * One page of the stored messages of [chatId] that contain [foldedQuery], newest first. The database gives the
 * messages that can match, and this function keeps the ones that match without case and diacritics.
 */
internal suspend fun searchHostStore(
    candidates: ChatRoomSearchCandidates,
    chatId: ChatId,
    foldedQuery: String,
    cursor: RoomSearchCursor.HostStore?,
    pageSize: Int = HOST_SEARCH_PAGE_SIZE,
    chunkSize: Int = HOST_SEARCH_CHUNK_SIZE,
): RoomSearchPage.Hits {
    if (foldedQuery.isEmpty()) return RoomSearchPage.Hits(emptyList(), next = null)

    val hits = mutableListOf<RoomSearchHit>()
    var before = cursor?.let { it.timestamp to it.messageId }
    while (true) {
        val chunk = candidates.candidates(chatId, foldedQuery, before, chunkSize)
        for (candidate in chunk) {
            if (searchMatches(candidate.text, foldedQuery).isEmpty()) continue

            hits += RoomSearchHit(candidate.messageId, candidate.timestamp, candidate.text, candidate.author())
            if (hits.size == pageSize) {
                return RoomSearchPage.Hits(hits, RoomSearchCursor.HostStore(candidate.timestamp, candidate.messageId))
            }
        }
        if (chunk.size < chunkSize) return RoomSearchPage.Hits(hits, next = null)

        val last = chunk.last()
        before = last.timestamp to last.messageId
    }
}

private fun ChatRoomSearchCandidate.author(): RoomSearchAuthor = when {
    byUser -> RoomSearchAuthor.User
    participantId != null -> participantAuthor(participantId)
    else -> RoomSearchAuthor.Room
}

private fun ProductRoomMessage.toHit() = RoomSearchHit(
    messageId = messageId,
    timestamp = timestamp,
    text = content.searchText(),
    author = participantAuthor(participantId),
)

private fun participantAuthor(participantId: String): RoomSearchAuthor = when (participantId) {
    ProductParticipant.USER -> RoomSearchAuthor.User
    ProductParticipant.PRODUCT -> RoomSearchAuthor.Room
    else -> RoomSearchAuthor.Participant(participantId)
}

/** The text of a hit of the product, as the host shows it in the result list. */
private fun ChatMessage.Content.searchText(): String = when (this) {
    is ChatMessage.Content.Text -> text
    is ChatMessage.Content.RichText -> text.orEmpty()
    is ChatMessage.Content.ProductBody -> body.previewText().orEmpty()
    is ChatMessage.Content.Edited -> content.text.orEmpty()
    else -> ""
}
