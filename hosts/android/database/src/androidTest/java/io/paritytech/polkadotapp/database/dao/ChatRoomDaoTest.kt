package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import io.paritytech.polkadotapp.database.model.MessageRevisionLocal
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ChatRoomDaoTest {

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
    fun previewShowsOriginalContentWhenMessageHasNoRevisions() = runBlocking<Unit> {
        withMessage(id = "m1", timestamp = 1, content = "original")

        val preview = lastMessagePreview()

        assertArrayEquals("original".encodeToByteArray(), preview.content)
    }

    @Test
    fun previewShowsLatestRevisionContentWhenMessageWasEdited() = runBlocking<Unit> {
        withMessage(id = "m1", timestamp = 1, content = "original")
        withRevision(messageId = "m1", timestamp = 2, content = "first edit")
        withRevision(messageId = "m1", timestamp = 3, content = "second edit")

        val preview = lastMessagePreview()

        assertArrayEquals("second edit".encodeToByteArray(), preview.content)
        assertEquals(ChatMessageLocal.Type.RICH_TEXT, preview.type)
    }

    @Test
    fun previewStaysOnLastContentMessageWhenNewerEditCarrierArrives() = runBlocking<Unit> {
        withMessage(id = "m1", timestamp = 1, content = "original")
        withMessage(id = "m2", timestamp = 2, content = "edit carrier", type = ChatMessageLocal.Type.EDITED)
        withRevision(messageId = "m1", timestamp = 2, content = "edited text")

        val preview = lastMessagePreview()

        assertEquals("m1", preview.id)
        assertArrayEquals("edited text".encodeToByteArray(), preview.content)
    }

    @Test
    fun chatWithTheLaterStoredMessageComesFirstEvenWhenTheSenderClockRanAhead() = runBlocking<Unit> {
        val otherChatId = byteArrayOf(0x03)
        val emptyChatId = byteArrayOf(0x04)
        database.chatRoomDao().insert(ChatRoomLocal(id = otherChatId, createdAt = 0, name = null, icon = null))
        database.chatRoomDao().insert(ChatRoomLocal(id = emptyChatId, createdAt = 10_000, name = null, icon = null))
        withMessage(id = "received", timestamp = 5_000, content = "received", chatId = chatId)
        withMessage(id = "sent", timestamp = 1_000, content = "sent", chatId = otherChatId)

        val order = database.chatRoomDao().subscribeChatSummaries().first().map { it.chatId.toList() }

        assertEquals(listOf(otherChatId, chatId, emptyChatId).map { it.toList() }, order)
    }

    private suspend fun withMessage(
        id: String,
        timestamp: Long,
        content: String,
        type: ChatMessageLocal.Type = ChatMessageLocal.Type.TEXT,
        chatId: ByteArray = this.chatId,
    ) {
        database.chatMessageDao().saveMessage(
            ChatMessageLocal(
                id = id,
                chatId = chatId,
                timestamp = timestamp,
                updatedAt = 0,
                sortOrder = ChatMessageLocal.UNORDERED,
                origin = ChatMessageLocal.Origin(type = ChatMessageLocal.OriginType.CONTACT, key = byteArrayOf(0x02)),
                status = ChatMessageLocal.Status.NEW,
                type = type,
                searchableContent = "",
                content = content.encodeToByteArray(),
                replyToMessageId = null,
                isInternal = false
            ),
            placement = ChatMessageDao.Placement.Latest,
            onSaved = {},
        )
    }

    private suspend fun withRevision(messageId: String, timestamp: Long, content: String) {
        database.messageRevisionDao().insert(
            MessageRevisionLocal(
                messageId = messageId,
                type = ChatMessageLocal.Type.RICH_TEXT,
                content = content.encodeToByteArray(),
                chatId = chatId,
                timestamp = timestamp
            )
        )
    }

    private suspend fun lastMessagePreview(): ChatMessageLocal {
        val summaries = database.chatRoomDao().subscribeChatSummaries().first()
        assertEquals(1, summaries.size)
        return requireNotNull(summaries.single().lastMessage)
    }
}
