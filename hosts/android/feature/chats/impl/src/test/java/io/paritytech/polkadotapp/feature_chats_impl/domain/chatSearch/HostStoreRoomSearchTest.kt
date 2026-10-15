package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidate
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidates
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HostStoreRoomSearchTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")

    @Test
    fun `a page has the 50 newest matches and a cursor after the last one`() = runBlocking<Unit> {
        val store = FakeCandidates((1..120).map { message(it, if (it % 2 == 0) "Crème $it" else "Plain $it") })

        val first = searchHostStore(store, room, foldQuery("creme"), cursor = null)
        val second = searchHostStore(store, room, foldQuery("creme"), first.next as RoomSearchCursor.HostStore)

        assertEquals((120 downTo 22 step 2).map { "m$it" }, first.hits.map { it.messageId })
        assertEquals(RoomSearchCursor.HostStore(timestamp = 22, messageId = "m22"), first.next)
        assertEquals((20 downTo 2 step 2).map { "m$it" }, second.hits.map { it.messageId })
        assertNull(second.next)
    }

    @Test
    fun `the database candidates that do not match without case and diacritics are left out`() = runBlocking<Unit> {
        // The database gives each message that has a char outside ASCII, so "Ölé" comes, but does not match.
        val store = FakeCandidates(listOf(message(1, "Ölé"), message(2, "CRÈME"), message(3, "cream")))

        val page = searchHostStore(store, room, foldQuery("crème"), cursor = null)

        assertEquals(listOf("m2"), page.hits.map { it.messageId })
    }

    @Test
    fun `the search reads the store in chunks until the page is full`() = runBlocking<Unit> {
        // Each "hay" message has a char outside ASCII, so the database gives it, and the search must read on.
        val store = FakeCandidates((1..1_000).map { message(it, if (it <= 3) "needle $it" else "hay ü $it") })

        val page = searchHostStore(store, room, foldQuery("needle"), cursor = null, chunkSize = 100)

        assertEquals(listOf("m3", "m2", "m1"), page.hits.map { it.messageId })
        assertNull(page.next)
        assertEquals(11, store.reads)
    }

    @Test
    fun `the author of a hit is the user, a participant or the room`() = runBlocking<Unit> {
        val store = FakeCandidates(
            listOf(
                message(1, "x", byUser = true),
                message(2, "x", participantId = "p1"),
                message(3, "x"),
            ),
        )

        val page = searchHostStore(store, room, foldQuery("x"), cursor = null)

        assertEquals(
            listOf(RoomSearchAuthor.Room, RoomSearchAuthor.Participant("p1"), RoomSearchAuthor.User),
            page.hits.map { it.author },
        )
    }

    private fun message(index: Int, text: String, byUser: Boolean = false, participantId: String? = null) =
        ChatRoomSearchCandidate("m$index", timestamp = index.toLong(), text = text, byUser = byUser, participantId = participantId)
}

/** The store of one room. It gives the candidates as the database query does: newest first, after the cursor. */
internal class FakeCandidates(messages: List<ChatRoomSearchCandidate>) : ChatRoomSearchCandidates {
    private val newestFirst = messages.sortedWith(compareByDescending<ChatRoomSearchCandidate> { it.timestamp }.thenByDescending { it.messageId })
    var reads = 0
        private set

    override suspend fun candidates(chatId: ChatId, foldedQuery: String, before: Pair<Long, ChatMessageId>?, limit: Int): List<ChatRoomSearchCandidate> {
        reads++
        return newestFirst
            .asSequence()
            .filter { before == null || it.timestamp < before.first || (it.timestamp == before.first && it.messageId < before.second) }
            .filter { it.text.contains(foldedQuery, ignoreCase = true) || it.text.any { char -> char.code !in 0x20..0x7e } }
            .take(limit)
            .toList()
    }
}
