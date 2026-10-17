package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStageException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStaging
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentDraft
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentFailure
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ProductAttachmentStagerTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val key = ByteArray(32) { 9 }.toDataByteArray()
    private val sealed = ChatResourceModel.Sealed(
        parts = listOf(ByteArray(32) { 1 }.toDataByteArray(), ByteArray(32) { 2 }.toDataByteArray()),
        key = ByteArray(32) { 3 }.toDataByteArray(),
        nonce = ByteArray(24) { 4 }.toDataByteArray(),
        size = 3L * 1024 * 1024 + 16,
        digest = ByteArray(64) { 5 }.toDataByteArray(),
    )
    private val bytes = ByteArray(3 * 1024 * 1024) { (it % 251).toByte() }

    private val outbox by lazy { ProductAttachmentOutbox(folder.root, Dispatchers.Unconfined) { 1_000L } }
    private val staging = FakeStaging()
    private val scheduler = FakeScheduler()
    private val rooms = FakeRooms()
    private val feedListeners = mockk<ProductFeedListeners>(relaxed = true)
    private val stager by lazy { ProductAttachmentStager(outbox, staging, scheduler, rooms, feedListeners) }

    @Test
    fun `a host-store room stores the message with the preimage and empties the outbox`() = runBlocking {
        val entry = stager.enqueue(room, draft(), text = "a caption", replyTo = "reply-1")
        assertEquals(listOf(entry.id), scheduler.scheduled)

        stager.process(entry.id)

        val (chatId, content, replyTo) = rooms.saved.single()
        assertEquals(room, chatId)
        assertEquals("reply-1", replyTo)
        val body = (content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText
        assertEquals("a caption", body.text)
        val image = body.media.single() as ChatMediaItemModel.Image
        assertEquals(ChatResourceModel.Preimage(key), image.resource)
        assertEquals(bytes.size.toLong(), image.meta.sizeBytes)
        assertArrayEquals(bytes, staging.staged.single())
        assertEquals(1, staging.holds)
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a product-source room sends NewMessage and shows the message that the product made`() = runBlocking {
        val querying = mockk<ProductChatQuerying>()
        val created = productMessage("m-7")
        coEvery { querying.userAction(room, any()) } returns Result.success(ChatUserActionOutcome.MessageCreated(created))
        rooms.policy = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)
        rooms.querying = querying
        val entry = stager.enqueue(room, draft(kind = ProductAttachmentDraft.Kind.FILE), text = null, replyTo = null)

        stager.process(entry.id)

        coVerify {
            querying.userAction(
                room,
                match { action ->
                    val media = ((action as ChatUserActionModel.NewMessage).content as ChatMessage.Content.ProductBody).body
                        .let { (it as ProductMessageBody.RichText).media.single() as ChatMediaItemModel.File }
                    media.resource == ChatResourceModel.Preimage(key) && media.fileName == "report.pdf"
                },
            )
        }
        coVerify { feedListeners.deliver(room, ProductFeedUpdateModel.New(created)) }
        assertTrue(rooms.saved.isEmpty())
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a refusal of the core keeps the entry with the reason and sends nothing`() = runBlocking {
        val refusals = mapOf(
            ProductAttachmentStageException.TooLarge(2_097_152) to ProductAttachmentFailure.TooLarge(2_097_152),
            ProductAttachmentStageException.PermissionDenied() to ProductAttachmentFailure.PermissionDenied,
            ProductAttachmentStageException.NotDeclared() to ProductAttachmentFailure.NotDeclared,
            ProductAttachmentStageException.AllowanceUnavailable("no slot") to ProductAttachmentFailure.AllowanceUnavailable("no slot"),
            ProductAttachmentStageException.SubmitFailed("timeout") to ProductAttachmentFailure.Failed("timeout"),
        )

        for ((refusal, failure) in refusals) {
            staging.result = Result.failure(refusal)
            val entry = stager.enqueue(room, draft(), text = null, replyTo = null)
            val event = async(start = CoroutineStart.UNDISPATCHED) { stager.observeFailures(room).first() }

            stager.process(entry.id)

            assertEquals(failure, outbox.get(entry.id)!!.failure)
            assertEquals(ProductAttachmentFailureEvent(room, entry.id, failure), withTimeout(5_000) { event.await() })
        }
        assertTrue(rooms.saved.isEmpty())
    }

    @Test
    fun `retry stages again and sends`() = runBlocking {
        staging.result = Result.failure(ProductAttachmentStageException.SubmitFailed("timeout"))
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)
        stager.process(entry.id)

        staging.result = Result.success(key)
        assertTrue(stager.retry(entry.id))
        assertEquals(listOf(entry.id, entry.id), scheduler.scheduled)
        stager.process(entry.id)

        assertEquals(2, staging.staged.size)
        assertEquals(1, rooms.saved.size)
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a room that seals sends the sealed file and stages no preimage`() = runBlocking {
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null, seals = true)

        stager.process(entry.id)

        val body = (rooms.saved.single().second as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText
        assertEquals(sealed, body.media.single().resource)
        assertArrayEquals(bytes, staging.sealedFiles.single())
        assertTrue(staging.staged.isEmpty())
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a sealed file that was not delivered is kept, so a retry seals nothing again`() = runBlocking {
        rooms.saveFailure = IllegalStateException("database closed")
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null, seals = true)
        stager.process(entry.id)
        val reread = ProductAttachmentOutbox(folder.root, Dispatchers.Unconfined) { 1_000L }.get(entry.id)!!
        assertEquals(sealed, reread.sealed)
        assertTrue(reread.seals)

        rooms.saveFailure = null
        stager.retry(entry.id)
        stager.process(entry.id)

        assertEquals(1, staging.sealedFiles.size)
        assertEquals(1, rooms.saved.size)
    }

    @Test
    fun `a retry after a failed delivery sends the message without a second upload`() = runBlocking {
        rooms.saveFailure = IllegalStateException("database closed")
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)
        stager.process(entry.id)
        val failed = outbox.get(entry.id)!!
        assertEquals(ProductAttachmentFailure.Failed("database closed"), failed.failure)
        assertEquals(key, failed.key)

        rooms.saveFailure = null
        stager.retry(entry.id)
        stager.process(entry.id)

        assertEquals(1, staging.staged.size)
        assertEquals(1, rooms.saved.size)
    }

    @Test
    fun `a product that refuses the message gives its reason`() = runBlocking {
        val querying = mockk<ProductChatQuerying>()
        coEvery { querying.userAction(room, any()) } returns Result.failure(ProductChatQueryException.Rejected("no files"))
        rooms.policy = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)
        rooms.querying = querying
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)

        stager.process(entry.id)

        assertEquals(ProductAttachmentFailure.Refused("no files"), outbox.get(entry.id)!!.failure)
    }

    @Test
    fun `retry does nothing for an entry that did not fail`() = runBlocking {
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)

        assertEquals(false, stager.retry(entry.id))
        assertEquals(false, stager.retry("outbox:unknown"))
        assertEquals(listOf(entry.id), scheduler.scheduled)
    }

    @Test
    fun `delete cancels the job and removes the kept bytes`() = runBlocking {
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)

        stager.delete(entry.id)
        stager.process(entry.id)

        assertEquals(listOf(entry.id), scheduler.cancelled)
        assertNull(outbox.get(entry.id))
        assertNull(outbox.bytes(entry.id))
        assertTrue(staging.staged.isEmpty())
    }

    @Test
    fun `the window shows the outbox at the newest end with Preparing and Failed`() = runBlocking {
        val sending = stager.enqueue(room, draft(), text = "first", replyTo = null)
        staging.result = Result.failure(ProductAttachmentStageException.PermissionDenied())
        val failed = stager.enqueue(room, draft(), text = "second", replyTo = null)
        stager.process(failed.id)
        val window = io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot(
            messages = emptyList(),
            reactions = emptyMap(),
            revisions = emptyMap(),
            productStatuses = emptyMap(),
            paging = io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState.Complete,
            readiness = io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness.READY,
        )

        val shown = window.withOutbox(stager.observe(room).first(), stager::preview)

        assertEquals(setOf(sending.id, failed.id), shown.messages.map { it.id }.toSet())
        assertEquals(ProductMessageStatusModel.PREPARING, shown.productStatuses[sending.id])
        assertEquals(ProductMessageStatusModel.FAILED, shown.productStatuses[failed.id])
        val preview = ((shown.messages.first().content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText)
            .media.single().resource as ChatResourceModel.Url
        assertTrue(preview.url.startsWith("file://"))
    }

    private fun draft(kind: ProductAttachmentDraft.Kind = ProductAttachmentDraft.Kind.IMAGE) = ProductAttachmentDraft(
        kind = kind,
        bytes = bytes,
        meta = ChatMediaMetaModel(mimeType = "image/jpeg", sizeBytes = bytes.size.toLong(), width = 4_000, height = 3_000, placeholder = null, alt = null),
        fileName = if (kind == ProductAttachmentDraft.Kind.FILE) "report.pdf" else null,
    )

    private fun productMessage(id: String) = ProductRoomMessage(
        messageId = id,
        participantId = ProductParticipant.USER,
        timestamp = 1L,
        content = ChatMessage.Content.Text("x"),
        status = ProductMessageStatusModel.SENT,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
    )

    private inner class FakeStaging : ProductAttachmentStaging {
        var result: Result<DataByteArray> = Result.success(key)
        val staged = mutableListOf<ByteArray>()
        val sealedFiles = mutableListOf<ByteArray>()
        var holds = 0

        override suspend fun <T> holdingWorker(chatId: ChatId, block: suspend () -> T): T {
            holds++
            return block()
        }

        override suspend fun stage(chatId: ChatId, mimeType: String, bytes: ByteArray): Result<DataByteArray> {
            staged += bytes
            return result
        }

        override suspend fun seal(chatId: ChatId, bytes: ByteArray): Result<ChatResourceModel.Sealed> {
            sealedFiles += bytes
            return Result.success(sealed)
        }
    }

    private class FakeScheduler : ProductAttachmentStageScheduler {
        val scheduled = mutableListOf<ChatMessageId>()
        val cancelled = mutableListOf<ChatMessageId>()

        override fun schedule(entryId: ChatMessageId) {
            scheduled += entryId
        }

        override fun cancel(entryId: ChatMessageId) {
            cancelled += entryId
        }
    }

    private class FakeRooms : ProductAttachmentRooms {
        var policy: ChatRoomPolicy? = ChatRoomPolicy.Defaults
        var querying: ProductChatQuerying? = null
        var saveFailure: Exception? = null
        val saved = mutableListOf<Triple<ChatId, ChatMessage.Content, ChatMessageId?>>()

        override suspend fun policy(chatId: ChatId): ChatRoomPolicy? = policy

        override suspend fun querying(chatId: ChatId): ProductChatQuerying? = querying

        override suspend fun saveUserMessage(chatId: ChatId, content: ChatMessage.Content, replyTo: ChatMessageId?) {
            saveFailure?.let { throw it }
            saved += Triple(chatId, content, replyTo)
        }
    }
}
