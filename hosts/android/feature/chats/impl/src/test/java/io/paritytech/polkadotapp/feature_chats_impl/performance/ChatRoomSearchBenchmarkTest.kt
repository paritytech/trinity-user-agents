package io.paritytech.polkadotapp.feature_chats_impl.performance

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidate
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidates
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchPage
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.foldQuery
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.searchHostStore
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import kotlin.system.measureNanoTime

/**
 * The Android number of 04-TESTING B8 on the JVM: the first page of an in-room search in a room of 50,000 stored
 * messages. A store in memory stands in for the database query: it gives the same candidates in the same order, so
 * the number is the work of the host on the candidates, without the read of SQLite. The suite runs only with
 * `CHAT_V2_BENCH=1` and prints `[bench]` lines.
 */
class ChatRoomSearchBenchmarkTest {
    private val room = ChatId.forExtensionRoom("ProductBot_bench.dot", "host")

    @Before
    fun onlyOnRequest() = assumeTrue("Set CHAT_V2_BENCH=1 to run the benchmarks", System.getenv("CHAT_V2_BENCH") == "1")

    @Test
    fun `B8 first results of an in-room search in a 50,000-message room`() = runBlocking<Unit> {
        val store = MemoryStore(corpus(MESSAGE_COUNT))
        val queries = mapOf(
            "a word of every message (\"message\")" to "message",
            "a word with diacritics in 1 of 80 messages (\"creme\")" to "creme",
            "a word of the 3 oldest messages (\"needle\")" to "needle",
            "a word of no message (\"zzzz\")" to "zzzz",
        )

        queries.forEach { (label, query) ->
            val folded = foldQuery(query)
            repeat(WARM_UP) { searchHostStore(store, room, folded, cursor = null) }
            store.read = 0
            var page: RoomSearchPage.Hits? = null
            val runs = (1..RUNS).map {
                measureNanoTime { page = searchHostStore(store, room, folded, cursor = null) } / 1_000_000.0
            }.sorted()
            val median = runs[runs.size / 2]
            println(
                "[bench] B8 %s: median %.1f ms, max %.1f ms, %d hits, %d stored messages scanned per search".format(
                    label, median, runs.last(), page!!.hits.size, store.read / RUNS,
                ),
            )
            assertTrue("B8 target is 300 ms for the first results: $label took $median ms", median < TARGET_MS)
        }
    }

    // About 1 message in 10 has a word with diacritics, and every message has the word "message".
    private fun corpus(count: Int): List<ChatRoomSearchCandidate> = (count downTo 1).map { index ->
        val words = WORDS.shuffled(kotlin.random.Random(index)).take(8).joinToString(" ")
        val extra = when {
            index <= 3 -> " needle"
            index % 10 == 0 -> " " + ACCENTED[index / 10 % ACCENTED.size]
            else -> ""
        }
        ChatRoomSearchCandidate(
            messageId = "m%06d".format(index),
            timestamp = index.toLong(),
            text = "Benchmark message $index: $words$extra",
            byUser = index % 2 == 0,
            participantId = null,
        )
    }

    /** Gives candidates as the query of `ChatMessageDao.searchMessageCandidatesInChat` does, with a keyset start. */
    private class MemoryStore(private val newestFirst: List<ChatRoomSearchCandidate>) : ChatRoomSearchCandidates {
        var read = 0

        override suspend fun candidates(
            chatId: ChatId,
            foldedQuery: String,
            before: Pair<Long, ChatMessageId>?,
            limit: Int,
        ): List<ChatRoomSearchCandidate> {
            // The timestamps are unique and go down, so the start is the first message older than the cursor.
            var index = if (before == null) 0 else newestFirst.binarySearch { before.first.compareTo(it.timestamp) }.let { if (it >= 0) it + 1 else -it - 1 }
            val chunk = ArrayList<ChatRoomSearchCandidate>(limit)
            while (index < newestFirst.size && chunk.size < limit) {
                val message = newestFirst[index++]
                read++
                if (message.text.contains(foldedQuery, ignoreCase = true) || message.text.any { it.code !in 0x20..0x7e }) {
                    chunk += message
                }
            }
            return chunk
        }
    }

    private companion object {
        const val MESSAGE_COUNT = 50_000
        const val WARM_UP = 5
        const val RUNS = 21
        const val TARGET_MS = 300.0

        val WORDS = listOf(
            "the", "room", "product", "host", "store", "search", "page", "worker", "chain", "token", "night", "river",
            "stone", "light", "green", "blue", "seven", "dinner", "travel", "window", "paper", "garden", "music", "coffee",
        )
        val ACCENTED = listOf("crème", "café", "naïve", "Zoë", "façade", "déjà", "Ångström", "über")
    }
}
