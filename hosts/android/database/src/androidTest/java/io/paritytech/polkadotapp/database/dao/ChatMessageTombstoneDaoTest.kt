package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatMessageReactionLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import io.paritytech.polkadotapp.database.model.MessageRevisionLocal
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ChatMessageTombstoneDaoTest {
    private val chatId = byteArrayOf(0x01)

    private lateinit var database: AppDatabase

    @Before
    fun setUp() = runBlocking<Unit> {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()
        database.chatRoomDao().insert(ChatRoomLocal(id = chatId, createdAt = 0, name = null, icon = null))
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun aDeletedMessageKeepsItsRowHiddenAndLosesItsReactionsAndRevisions() = runBlocking<Unit> {
        withMessage(id = "m1", timestamp = 1)
        withMessage(id = "m2", timestamp = 2)
        database.messageReactionsDao().insertReaction(
            ChatMessageReactionLocal("m2", "👍", ChatMessageLocal.Origin(ChatMessageLocal.OriginType.USER, null), chatId, 3)
        )
        database.messageRevisionDao().insert(MessageRevisionLocal("m2", ChatMessageLocal.Type.TEXT, byteArrayOf(9), chatId, 3))

        database.chatMessageDao().rewriteAsDeleted("m2", content = byteArrayOf(0x19), updatedAt = 4)

        val tombstone = requireNotNull(database.chatMessageDao().getMessage("m2"))
        assertEquals(ChatMessageLocal.Type.DELETED, tombstone.type)
        assertTrue(tombstone.isInternal)
        assertEquals(ChatMessageLocal.Status.IS_READ, tombstone.status)
        assertTrue(database.messageReactionsDao().subscribeReactions(chatId).first().isEmpty())
        assertNull(database.messageRevisionDao().getLatestRevision("m2"))

        val summary = database.chatRoomDao().subscribeChatSummaries().first().single()
        assertEquals("m1", summary.lastMessage?.id)
        assertEquals(1, summary.unseenCount)
    }

    @Test
    fun theNewestVisibleMessageSkipsHiddenOnesAndLaterOnes() = runBlocking<Unit> {
        withMessage(id = "m1", timestamp = 1)
        withMessage(id = "m2", timestamp = 2)
        withMessage(id = "m3", timestamp = 5)
        database.chatMessageDao().rewriteAsDeleted("m2", content = byteArrayOf(0x19), updatedAt = 4)

        assertEquals("m1", database.chatMessageDao().getNewestVisibleMessageIdUpTo(chatId, timestamp = 4))
        assertEquals("m3", database.chatMessageDao().getNewestVisibleMessageIdUpTo(chatId, timestamp = 5))
        assertNull(database.chatMessageDao().getNewestVisibleMessageIdUpTo(chatId, timestamp = 0))
    }

    @Test
    fun aMessageKeepsTheParticipantThatWroteIt() = runBlocking<Unit> {
        withMessage(id = "m1", timestamp = 1, participant = "alice")

        assertEquals("alice", database.chatMessageDao().getMessage("m1")?.originParticipantId)
    }

    private suspend fun withMessage(id: String, timestamp: Long, participant: String? = null) {
        database.chatMessageDao().saveMessage(
            ChatMessageLocal(
                id = id,
                chatId = chatId,
                timestamp = timestamp,
                updatedAt = 0,
                sortOrder = ChatMessageLocal.UNORDERED,
                origin = ChatMessageLocal.Origin(type = ChatMessageLocal.OriginType.MIDDLEWARE, key = byteArrayOf(0x02)),
                status = ChatMessageLocal.Status.NEW,
                type = ChatMessageLocal.Type.TEXT,
                searchableContent = "",
                content = byteArrayOf(0x00),
                replyToMessageId = null,
                isInternal = false,
                originParticipantId = participant,
            ),
            placement = ChatMessageDao.Placement.Latest,
            onSaved = {},
        )
    }
}
