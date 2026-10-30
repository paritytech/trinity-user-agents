package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.dao.ChatMessageDao.Placement
import io.paritytech.polkadotapp.database.model.ChatMessageLocal
import io.paritytech.polkadotapp.database.model.ChatRoomLocal
import io.paritytech.polkadotapp.database.model.FileUploadLocal
import io.paritytech.polkadotapp.database.model.TransferRetryStateLocal
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class FileUploadDaoTest {

    private val chatId = byteArrayOf(0x01)

    private lateinit var database: AppDatabase
    private val dao get() = database.fileUploadDao()

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
    fun claimingAnUploadMarksItInProgress() = runBlocking<Unit> {
        givenUpload("a")

        dao.claimNextDue(NOW)

        assertEquals(FileUploadLocal.Status.IN_PROGRESS, statusOf("a"))
    }

    @Test
    fun claimingAnUploadReturnsItInProgress() = runBlocking<Unit> {
        givenUpload("a")

        val claimed = dao.claimNextDue(NOW)

        assertEquals("a", claimed?.messageId)
        assertEquals(FileUploadLocal.Status.IN_PROGRESS, claimed?.status)
    }

    @Test
    fun concurrentClaimsNeverTakeTheSameUpload() = runBlocking<Unit> {
        val ids = List(CONCURRENT_CLAIMS) { "upload-$it" }
        ids.forEach { givenUpload(it) }

        val claimed = List(CONCURRENT_CLAIMS) { async(Dispatchers.IO) { dao.claimNextDue(NOW)?.messageId } }.awaitAll()

        assertEquals(ids.toSet(), claimed.toSet())
    }

    @Test
    fun aClaimedUploadIsNotClaimedAgain() = runBlocking<Unit> {
        givenUpload("a", createdAt = 1)
        givenUpload("b", createdAt = 2)

        val first = dao.claimNextDue(NOW)
        val second = dao.claimNextDue(NOW)
        val third = dao.claimNextDue(NOW)

        assertEquals(listOf("a", "b"), listOf(first?.messageId, second?.messageId))
        assertNull(third)
    }

    @Test
    fun anUploadLeftInProgressIsNotClaimed() = runBlocking<Unit> {
        givenUpload("a", status = FileUploadLocal.Status.IN_PROGRESS)

        val claimed = dao.claimNextDue(NOW)

        assertNull(claimed)
    }

    @Test
    fun anUploadLeftInProgressIsClaimedOnceReleased() = runBlocking<Unit> {
        givenUpload("a", status = FileUploadLocal.Status.IN_PROGRESS)
        dao.releaseInProgress()

        val claimed = dao.claimNextDue(NOW)

        assertEquals("a", claimed?.messageId)
    }

    @Test
    fun aFinishedUploadIsNeverClaimed() = runBlocking<Unit> {
        givenUpload("done", status = FileUploadLocal.Status.DONE)
        givenUpload("failed", status = FileUploadLocal.Status.FAILED)

        val claimed = dao.claimNextDue(NOW)

        assertNull(claimed)
    }

    @Test
    fun anUploadWaitingOutItsBackoffIsNotClaimed() = runBlocking<Unit> {
        givenUpload("a", nextAttemptAt = NOW + 1)

        val claimed = dao.claimNextDue(NOW)

        assertNull(claimed)
    }

    @Test
    fun releasingKeepsTheNodeAndChunksOfAnUploadLeftInProgress() = runBlocking<Unit> {
        givenUpload("a", status = FileUploadLocal.Status.IN_PROGRESS, nodeUrl = NODE_A, chunkHashes = HASHES)

        dao.releaseInProgress()

        assertEquals(FileUploadLocal.Status.PENDING, statusOf("a"))
        assertResumesWithChunks("a", NODE_A)
    }

    @Test
    fun releasingLeavesFinishedUploadsAlone() = runBlocking<Unit> {
        givenUpload("done", status = FileUploadLocal.Status.DONE)
        givenUpload("failed", status = FileUploadLocal.Status.FAILED)

        dao.releaseInProgress()

        assertEquals(FileUploadLocal.Status.DONE, statusOf("done"))
        assertEquals(FileUploadLocal.Status.FAILED, statusOf("failed"))
    }

    @Test
    fun aRetryOnTheSameNodeKeepsItsChunks() = runBlocking<Unit> {
        givenUpload("a", status = FileUploadLocal.Status.IN_PROGRESS, nodeUrl = NODE_A, chunkHashes = HASHES)

        scheduleRetry("a", nodeUrl = NODE_A)

        assertEquals(FileUploadLocal.Status.PENDING, statusOf("a"))
        assertResumesWithChunks("a", NODE_A)
    }

    @Test
    fun aRetryOnAnotherNodeDropsItsChunks() = runBlocking<Unit> {
        givenUpload("a", status = FileUploadLocal.Status.IN_PROGRESS, nodeUrl = NODE_A, chunkHashes = HASHES)

        scheduleRetry("a", nodeUrl = NODE_B)

        assertEquals(FileUploadLocal.Status.PENDING, statusOf("a"))
        assertStartsOverOn("a", NODE_B)
    }

    private suspend fun scheduleRetry(messageId: String, nodeUrl: String) {
        dao.scheduleRetry(messageId, nodeUrl, attemptCount = 2, firstFailureAt = 0, nextAttemptAt = NOW)
    }

    private suspend fun statusOf(messageId: String) = dao.get(messageId)?.status

    private suspend fun assertResumesWithChunks(messageId: String, nodeUrl: String) {
        val upload = requireNotNull(dao.get(messageId))
        assertEquals(nodeUrl, upload.nodeUrl)
        assertEquals(HASHES, upload.chunkHashes)
        assertEquals(HASHES.split(",").size, upload.uploadedChunks)
    }

    private suspend fun assertStartsOverOn(messageId: String, nodeUrl: String) {
        val upload = requireNotNull(dao.get(messageId))
        assertEquals(nodeUrl, upload.nodeUrl)
        assertNull(upload.chunkHashes)
        assertEquals(0, upload.uploadedChunks)
    }

    private suspend fun givenUpload(
        messageId: String,
        status: FileUploadLocal.Status = FileUploadLocal.Status.PENDING,
        createdAt: Long = 0,
        nextAttemptAt: Long? = null,
        nodeUrl: String = NODE_A,
        chunkHashes: String? = null
    ) {
        database.chatMessageDao().saveMessage(message(messageId), Placement.Latest, onSaved = {})
        dao.insert(
            FileUploadLocal(
                messageId = messageId,
                chatId = chatId,
                fileUri = "content://upload/$messageId",
                mimeType = "video/mp4",
                fileSize = null,
                totalChunks = 3,
                uploadedChunks = chunkHashes?.split(",")?.size ?: 0,
                ticket = byteArrayOf(0x04),
                nodeUrl = nodeUrl,
                chunkHashes = chunkHashes,
                status = status,
                errorCategory = null,
                errorCause = null,
                createdAt = createdAt,
                retryState = TransferRetryStateLocal(attemptCount = 0, firstFailureAt = null, nextAttemptAt = nextAttemptAt)
            )
        )
    }

    private fun message(id: String) = ChatMessageLocal(
        id = id,
        chatId = chatId,
        timestamp = 0,
        updatedAt = 0,
        sortOrder = ChatMessageLocal.UNORDERED,
        origin = ChatMessageLocal.Origin(type = ChatMessageLocal.OriginType.USER, key = null),
        status = ChatMessageLocal.Status.IS_SENT,
        type = ChatMessageLocal.Type.TEXT,
        searchableContent = "",
        content = id.encodeToByteArray(),
        replyToMessageId = null,
        isInternal = false
    )

    private companion object {
        const val NOW = 1_000_000L
        const val CONCURRENT_CLAIMS = 20
        const val NODE_A = "wss://node-a"
        const val NODE_B = "wss://node-b"
        const val HASHES = "0xaa,0xbb"
    }
}
