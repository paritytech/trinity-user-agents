package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.dao.ChatMessageDao.Placement
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ChatMessageDaoOrderTest {

    private val chatId = byteArrayOf(0x01)
    private val otherChatId = byteArrayOf(0x03)
    private val now = 1_000_000L
    private val senderClockAhead = now + 60_000

    private lateinit var database: AppDatabase
    private val dao get() = database.chatMessageDao()

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
    fun sentMessageAppearsAfterReceivedOneWhenSenderClockRunsAhead() = runBlocking<Unit> {
        saveIgnoring(received(id = "received", timestamp = senderClockAhead))
        saveReplacing(sent(id = "sent", timestamp = now))

        assertEquals(listOf("received", "sent"), feed())
    }

    @Test
    fun chatPreviewShowsTheLaterStoredMessageWhenSenderClockRunsAhead() = runBlocking<Unit> {
        saveIgnoring(received(id = "received", timestamp = senderClockAhead))
        saveReplacing(sent(id = "sent", timestamp = now))

        assertEquals("sent", lastMessageId())
    }

    @Test
    fun replacingAMessageKeepsItsPlace() = runBlocking<Unit> {
        saveReplacing(sent(id = "first", timestamp = now))
        saveIgnoring(received(id = "second", timestamp = now + 1_000))

        saveReplacing(sent(id = "first", timestamp = now, status = ChatMessageLocal.Status.DELIVERY_FAILED))

        assertEquals(listOf("first", "second"), feed())
    }

    @Test
    fun ignoredDuplicateKeepsItsPlace() = runBlocking<Unit> {
        saveIgnoring(received(id = "first", timestamp = now))
        saveReplacing(sent(id = "second", timestamp = now + 1_000))

        saveIgnoring(received(id = "first", timestamp = now))

        assertEquals(listOf("first", "second"), feed())
    }

    @Test
    fun batchIsPlacedBySenderTimestamps() = runBlocking<Unit> {
        saveReplacing(sent(id = "sent", timestamp = now))

        dao.saveMessagesIfNotExist(
            listOf(received(id = "later", timestamp = now + 2_000), received(id = "earlier", timestamp = now + 1_000)),
            Placement.Latest
        )

        assertEquals(listOf("sent", "earlier", "later"), feed())
    }

    @Test
    fun migratedRowsSortBeforeNewMessages() = runBlocking<Unit> {
        insertLegacy(received(id = "legacy", timestamp = senderClockAhead))

        saveReplacing(sent(id = "sent", timestamp = now))

        assertEquals(listOf("legacy", "sent"), feed())
    }

    @Test
    fun syncedBacklogMessageSortsByTimestampAmongLegacyHistory() = runBlocking<Unit> {
        insertLegacy(received(id = "legacy-old", timestamp = 1_000))
        insertLegacy(received(id = "legacy-new", timestamp = 3_000))
        saveReplacing(sent(id = "sent-now", timestamp = now))

        saveIgnoring(received(id = "synced-backlog", timestamp = 2_000), Placement.ByTimestamp)

        assertEquals(listOf("legacy-old", "synced-backlog", "legacy-new", "sent-now"), feed())
        assertEquals("sent-now", lastMessageId())
    }

    @Test
    fun syncedBacklogComparesWithTheChatsNewestMessageEvenWhenItIsInternal() = runBlocking<Unit> {
        insertLegacy(received(id = "displayed", timestamp = 1_000))
        insertLegacy(received(id = "internal", timestamp = 5_000, type = ChatMessageLocal.Type.DEVICE_ADDED, isInternal = true))

        saveIgnoring(sent(id = "synced", timestamp = 2_000), Placement.ByTimestamp)

        assertEquals(listOf("displayed", "synced", "internal"), feed())
    }

    @Test
    fun syncedBacklogMessageInterleavesOrderedHistoryByTimestamp() = runBlocking<Unit> {
        saveIgnoring(received(id = "older", timestamp = now))
        saveReplacing(sent(id = "newer", timestamp = now + 2_000))

        saveIgnoring(sent(id = "synced-between", timestamp = now + 1_000), Placement.ByTimestamp)

        assertEquals(listOf("older", "synced-between", "newer"), feed())
        assertEquals("newer", lastMessageId())
    }

    @Test
    fun syncedBacklogOlderThanTheWholeChatGoesFirst() = runBlocking<Unit> {
        saveIgnoring(received(id = "received", timestamp = now))

        saveIgnoring(sent(id = "synced-oldest", timestamp = now - 1_000), Placement.ByTimestamp)

        assertEquals(listOf("synced-oldest", "received"), feed())
    }

    @Test
    fun liveSyncedMessageLandsAtTheBottom() = runBlocking<Unit> {
        saveIgnoring(received(id = "received", timestamp = now))

        saveIgnoring(sent(id = "synced-live", timestamp = now + 1_000), Placement.ByTimestamp)

        assertEquals(listOf("received", "synced-live"), feed())
    }

    @Test
    fun syncedMessageIsPlacedAgainstItsOwnChatOnly() = runBlocking<Unit> {
        database.chatRoomDao().insert(ChatRoomLocal(id = otherChatId, createdAt = 0, name = null, icon = null))
        saveIgnoring(received(id = "received", timestamp = now))
        saveIgnoring(received(id = "other-chat", timestamp = senderClockAhead, chatId = otherChatId))

        saveIgnoring(sent(id = "synced-live", timestamp = now + 1_000), Placement.ByTimestamp)

        assertEquals(listOf("received", "synced-live"), feed())
    }

    @Test
    fun compactedMessagesTakeTheCommitsPlace() = runBlocking<Unit> {
        saveIgnoring(received(id = "commit", timestamp = now))
        saveReplacing(sent(id = "sent-while-downloading", timestamp = now + 5_000))

        dao.saveMessagesIfNotExist(
            listOf(received(id = "compacted-2", timestamp = now - 1_000), received(id = "compacted-1", timestamp = now - 2_000)),
            Placement.SameAs("commit")
        )

        assertEquals(listOf("compacted-1", "compacted-2", "commit", "sent-while-downloading"), feed())
    }

    @Test
    fun compactedMessagesGoLastWhenTheCommitIsGone() = runBlocking<Unit> {
        saveReplacing(sent(id = "sent", timestamp = now))

        dao.saveMessagesIfNotExist(listOf(received(id = "compacted", timestamp = now - 1_000)), Placement.SameAs("missing"))

        assertEquals(listOf("sent", "compacted"), feed())
    }

    @Test
    fun compactedBatchLargerThanTheSqliteVariableLimitIsStored() = runBlocking<Unit> {
        saveIgnoring(received(id = "commit", timestamp = now))
        val compacted = (1..1_500).map { index -> received(id = "compacted-$index", timestamp = now - index) }

        val rowIds = dao.saveMessagesIfNotExist(compacted, Placement.SameAs("commit"))

        assertEquals(compacted.size, rowIds.count { it >= 0 })
        assertEquals("compacted-1500", feed().first())
    }

    @Test
    fun storedBatchLargerThanTheSqliteVariableLimitKeepsItsPlaces() = runBlocking<Unit> {
        val batch = (1..1_500).map { index -> received(id = "received-$index", timestamp = now + index) }
        dao.saveMessagesIfNotExist(batch, Placement.Latest)
        saveReplacing(sent(id = "sent", timestamp = now))
        val before = feed()

        dao.saveMessages(batch, Placement.Latest)

        assertEquals(before, feed())
    }

    @Test
    fun batchKeepsStoredPlacesAndAppendsNewMessages() = runBlocking<Unit> {
        saveIgnoring(received(id = "first", timestamp = now))
        saveReplacing(sent(id = "second", timestamp = now + 1_000))

        dao.saveMessages(
            listOf(received(id = "third", timestamp = now + 2_000), received(id = "first", timestamp = now)),
            Placement.Latest
        )

        assertEquals(listOf("first", "second", "third"), feed())
    }

    @Test
    fun syncedBatchIsPlacedAgainstEachMessagesOwnChat() = runBlocking<Unit> {
        database.chatRoomDao().insert(ChatRoomLocal(id = otherChatId, createdAt = 0, name = null, icon = null))
        saveIgnoring(received(id = "received", timestamp = now))
        saveIgnoring(received(id = "other-received", timestamp = senderClockAhead, chatId = otherChatId))

        dao.saveMessagesIfNotExist(
            listOf(
                received(id = "synced-live", timestamp = now + 1_000),
                received(id = "synced-backlog", timestamp = now + 1_000, chatId = otherChatId),
            ),
            Placement.ByTimestamp
        )

        assertEquals(listOf("received", "synced-live"), feed())
        assertEquals(listOf("synced-backlog", "other-received"), feed(otherChatId))
    }

    private suspend fun saveReplacing(message: ChatMessageLocal) {
        dao.saveMessage(message, Placement.Latest, onSaved = {})
    }

    private suspend fun saveIgnoring(message: ChatMessageLocal, placement: Placement = Placement.Latest) {
        dao.saveMessageIfNotExists(message, placement)
    }

    private fun insertLegacy(message: ChatMessageLocal) {
        database.openHelper.writableDatabase.execSQL(
            """
            INSERT INTO chat_messages (id, chatId, timestamp, updatedAt, origintype, originkey, status, type, searchableContent, content, isInternal)
            VALUES (?, ?, ?, 0, ?, ?, ?, ?, '', ?, ?)
            """,
            arrayOf(
                message.id,
                message.chatId,
                message.timestamp,
                message.origin.type.name,
                message.origin.key,
                message.status.name,
                message.type.name,
                message.content,
                if (message.isInternal) 1 else 0,
            )
        )
    }

    private suspend fun feed(chatId: ByteArray = this.chatId): List<String> {
        return dao.subscribeMessages(chatId).first().map { it.id }.reversed()
    }

    private suspend fun lastMessageId(): String? {
        return database.chatRoomDao().subscribeChatSummaries().first().single { it.chatId.contentEquals(chatId) }.lastMessage?.id
    }

    private fun received(
        id: String,
        timestamp: Long,
        chatId: ByteArray = this.chatId,
        type: ChatMessageLocal.Type = ChatMessageLocal.Type.TEXT,
        isInternal: Boolean = false,
    ) = message(id, timestamp, chatId, ChatMessageLocal.OriginType.CONTACT, ChatMessageLocal.Status.NEW, type, isInternal)

    private fun sent(id: String, timestamp: Long, status: ChatMessageLocal.Status = ChatMessageLocal.Status.IS_SENT) =
        message(id, timestamp, chatId, ChatMessageLocal.OriginType.USER, status, ChatMessageLocal.Type.TEXT, isInternal = false)

    private fun message(
        id: String,
        timestamp: Long,
        chatId: ByteArray,
        originType: ChatMessageLocal.OriginType,
        status: ChatMessageLocal.Status,
        type: ChatMessageLocal.Type,
        isInternal: Boolean,
    ) = ChatMessageLocal(
        id = id,
        chatId = chatId,
        timestamp = timestamp,
        updatedAt = 0,
        sortOrder = ChatMessageLocal.UNORDERED,
        origin = ChatMessageLocal.Origin(type = originType, key = byteArrayOf(0x02).takeIf { originType != ChatMessageLocal.OriginType.USER }),
        status = status,
        type = type,
        searchableContent = "",
        content = id.encodeToByteArray(),
        replyToMessageId = null,
        isInternal = isInternal
    )
}
