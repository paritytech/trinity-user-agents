package io.paritytech.polkadotapp.database.dao

import android.util.Log
import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.system.measureNanoTime

/** The query of the in-room search (07-LATER §8.2): one room, newest first, a keyset cursor, and its prefilter. */
@RunWith(AndroidJUnit4::class)
class ChatMessageRoomSearchDaoTest {
    private val room = byteArrayOf(0x01)
    private val otherRoom = byteArrayOf(0x02)

    private lateinit var database: AppDatabase
    private val dao get() = database.chatMessageDao()

    @Before
    fun setUp() {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun theCandidatesAreTheMessagesOfTheRoomThatContainTheQueryOrACharOutsideAscii() = runBlocking<Unit> {
        dao.saveMessages(
            listOf(
                message("a", room, 1, "Plain CREME text"),
                message("b", room, 2, "Crème brûlée"),
                message("c", room, 3, "nothing here"),
                message("d", otherRoom, 4, "creme in another room"),
                message("e", room, 5, "creme edit", type = ChatMessageLocal.Type.EDITED),
                message("f", room, 6, "creme internal", isInternal = true),
                message("g", room, 7, "100% creme_x"),
            ),
            ChatMessageDao.Placement.Latest,
        )

        val rows = dao.searchMessageCandidatesInChat(room, "creme", Long.MAX_VALUE, "", limit = 10)

        assertEquals(listOf("g", "b", "a"), rows.map { it.id })
        assertEquals(ChatMessageLocal.OriginType.CONTACT, rows.first().originType)
        assertEquals("p1", rows.first().originParticipantId)
    }

    @Test
    fun theCursorStartsAfterTheLastCandidateAndAWildcardIsAChar() = runBlocking<Unit> {
        dao.saveMessages(
            listOf(
                message("a", room, 1, "50% off"),
                message("b", room, 2, "50 percent"),
                message("c", room, 2, "50% now"),
                message("d", room, 3, "50% later"),
            ),
            ChatMessageDao.Placement.Latest,
        )

        val first = dao.searchMessageCandidatesInChat(room, "50\\%", Long.MAX_VALUE, "", limit = 2)
        val next = dao.searchMessageCandidatesInChat(room, "50\\%", first.last().timestamp, first.last().id, limit = 2)

        assertEquals(listOf("d", "c"), first.map { it.id })
        assertEquals(listOf("a"), next.map { it.id })
    }

    // 04-TESTING B8 on the device: the database part of the first page in a room of 50,000 messages.
    @Test
    fun b8TheCandidatesOfAFirstPageIn50000MessagesComeFast() = runBlocking<Unit> {
        val messages = (1..50_000).map { index ->
            val extra = if (index % 80 == 0) " crème" else ""
            message("m$index", room, index.toLong(), "Benchmark message $index$extra")
        }
        messages.chunked(5_000).forEach { dao.saveMessages(it, ChatMessageDao.Placement.Latest) }

        val timings = listOf("message" to 200, "creme" to 200, "zzzz" to 200).map { (query, limit) ->
            var rows = 0
            val runs = (1..5).map { measureNanoTime { rows = dao.searchMessageCandidatesInChat(room, query, Long.MAX_VALUE, "", limit).size } / 1e6 }
            Log.i("B8", "[bench] B8 device SQLite candidates for \"$query\": ${"%.1f".format(runs.sorted()[2])} ms median, $rows rows")
            runs.sorted()[2]
        }

        assertTrue("B8: ${timings.max()} ms", timings.max() < 300.0)
    }

    private fun message(
        id: String,
        chatId: ByteArray,
        timestamp: Long,
        text: String,
        type: ChatMessageLocal.Type = ChatMessageLocal.Type.TEXT,
        isInternal: Boolean = false,
    ) = ChatMessageLocal(
        id = id,
        chatId = chatId,
        timestamp = timestamp,
        updatedAt = 0,
        sortOrder = ChatMessageLocal.UNORDERED,
        origin = ChatMessageLocal.Origin(ChatMessageLocal.OriginType.CONTACT, key = null),
        status = ChatMessageLocal.Status.IS_READ,
        type = type,
        searchableContent = text,
        content = byteArrayOf(),
        isInternal = isInternal,
        originParticipantId = "p1",
    )
}
