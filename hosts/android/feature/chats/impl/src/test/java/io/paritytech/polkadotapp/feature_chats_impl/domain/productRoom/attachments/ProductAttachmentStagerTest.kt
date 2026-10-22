package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
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
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ProductAttachmentStagerTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val room = ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-01")
    private val hopFile = ChatResourceModel.Hop(
        identifier = ByteArray(32) { 6 }.toDataByteArray(),
        ticket = ByteArray(32) { 7 }.toDataByteArray(),
        node = "wss://hop.example",
        size = 3L * 1024 * 1024,
    )
    private val bytes = ByteArray(3 * 1024 * 1024) { (it % 251).toByte() }

    private var clock = 1_000L
    private val outbox by lazy { ProductAttachmentOutbox(folder.root, Dispatchers.Unconfined) { clock } }
    private val staging = FakeStaging()
    private val scheduler = FakeScheduler()
    private val rooms = FakeRooms()
    private val feedListeners = mockk<ProductFeedListeners>(relaxed = true)
    private val hopCache = mockk<ChatHopFileCache>(relaxed = true)
    private val stager by lazy { ProductAttachmentStager(outbox, staging, scheduler, rooms, feedListeners, hopCache) }

    @Test
    fun `a host-store room stores the message with the HOP file and keeps its bytes, so this device never opens its own upload`() =
        runBlocking {
            val entry = stager.enqueue(room, draft(), text = "a caption", replyTo = "reply-1")
            assertEquals(listOf(room), scheduler.scheduled)

            assertTrue(stager.processRoom(room))

            val (chatId, content, replyTo) = rooms.saved.single()
            assertEquals(room, chatId)
            assertEquals("reply-1", replyTo)
            val body = (content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText
            assertEquals("a caption", body.text)
            val image = body.media.single() as ChatMediaItemModel.Image
            assertEquals(hopFile, image.resource)
            assertEquals(bytes.size.toLong(), image.meta.sizeBytes)
            assertArrayEquals(bytes, staging.hopFiles.single())
            assertEquals(1, staging.holds)
            coVerify { hopCache.keepUpload(hopFile, any(), bytes) }
            assertNull(outbox.get(entry.id))
        }

    @Test
    fun `a product-source room sends NewMessage under the id of the entry and shows the message that the product made`() =
        runBlocking {
            val querying = productRoom()
            val entry = stager.enqueue(room, draft(kind = ProductAttachmentDraft.Kind.FILE), text = null, replyTo = null)
            val created = productMessage(entry.id)
            coEvery { querying.userAction(room, any()) } returns Result.success(ChatUserActionOutcome.MessageCreated(created))

            assertTrue(stager.processRoom(room))

            coVerify {
                querying.userAction(
                    room,
                    match { action ->
                        val newMessage = action as ChatUserActionModel.NewMessage
                        val media = (newMessage.content as ChatMessage.Content.ProductBody).body
                            .let { (it as ProductMessageBody.RichText).media.single() as ChatMediaItemModel.File }
                        newMessage.messageId == entry.id && media.resource == hopFile && media.fileName == "report.pdf"
                    },
                )
            }
            coVerify { feedListeners.deliver(room, ProductFeedUpdateModel.New(created)) }
            assertTrue(rooms.saved.isEmpty())
            assertNull(outbox.get(entry.id))
        }

    @Test
    fun `NewMessage carries the time the user wrote the message`() = runBlocking {
        val querying = productRoom()
        clock = 4_242L
        val entry = stager.enqueueText(room, "hi", replyTo = null, request = false)
        clock = 9_000L
        coEvery { querying.userAction(room, any()) } returns Result.success(ChatUserActionOutcome.MessageCreated(productMessage(entry.id)))

        assertTrue(stager.processRoom(room))

        coVerify { querying.userAction(room, match { (it as ChatUserActionModel.NewMessage).timestamp == 4_242L }) }
    }

    @Test
    fun `an upload shows the bytes that the node holds while it runs, and nothing after it`() = runBlocking {
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)

        assertTrue(stager.processRoom(room))

        assertEquals(
            mapOf(entry.id to ProductAttachmentStager.UploadProgress(bytes.size / 2L, bytes.size.toLong())),
            staging.uploadsDuringUpload,
        )
        assertTrue(stager.observeUploads().first().isEmpty())
    }

    @Test
    fun `a message that did not go goes again under the same id, before the messages after it`() = runBlocking {
        val querying = productRoom()
        val first = stager.enqueueText(room, "first", replyTo = null, request = false)
        val second = stager.enqueueText(room, "second", replyTo = first.id, request = false)
        coEvery { querying.userAction(room, any()) } returns Result.failure(ProductChatQueryException.NoAnswer())

        assertFalse(stager.processRoom(room))

        // The native chat keeps the message pending, never failed, while it cannot go yet.
        assertNull(outbox.get(first.id)!!.failure)
        coVerify(exactly = 1) { querying.userAction(room, any()) }

        coEvery { querying.userAction(room, any()) } answers {
            val action = secondArg<ChatUserActionModel>() as ChatUserActionModel.NewMessage
            Result.success(ChatUserActionOutcome.MessageCreated(productMessage(action.messageId!!)))
        }
        assertTrue(stager.processRoom(room))

        coVerify(exactly = 2) { querying.userAction(room, match { (it as? ChatUserActionModel.NewMessage)?.messageId == first.id }) }
        coVerify { querying.userAction(room, match { it.isText("second", replyTo = first.id, messageId = second.id) }) }
        assertTrue(outbox.all().isEmpty())
    }

    @Test
    fun `an attachment the room cannot take fails, and a failed upload goes again`() = runBlocking {
        val refusals = mapOf(
            ProductAttachmentStageException.TooLarge(2_097_152) to ProductAttachmentFailure.TooLarge(2_097_152),
            ProductAttachmentStageException.NotDeclared() to ProductAttachmentFailure.NotDeclared,
        )
        for ((refusal, failure) in refusals) {
            staging.result = Result.failure(refusal)
            val entry = stager.enqueue(room, draft(), text = null, replyTo = null)

            assertTrue(stager.processRoom(room))

            assertEquals(failure, outbox.get(entry.id)!!.failure)
            stager.delete(entry.id)
        }

        staging.result = Result.failure(ProductAttachmentStageException.SubmitFailed("timeout"))
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)
        assertFalse(stager.processRoom(room))
        assertNull(outbox.get(entry.id)!!.failure)

        staging.result = Result.success(hopFile)
        assertTrue(stager.processRoom(room))
        assertEquals(1, rooms.saved.size)
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a HOP file that was not delivered is kept, so sending again uploads nothing again`() = runBlocking {
        rooms.saveFailure = IllegalStateException("database closed")
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)
        assertFalse(stager.processRoom(room))
        val reread = ProductAttachmentOutbox(folder.root, Dispatchers.Unconfined) { 1_000L }.get(entry.id)!!
        assertEquals(hopFile, reread.hopFile)
        assertNull(reread.failure)

        rooms.saveFailure = null
        assertTrue(stager.processRoom(room))

        assertEquals(1, staging.hopFiles.size)
        assertEquals(1, rooms.saved.size)
    }

    @Test
    fun `a message that the product refuses fails, and the next message goes`() = runBlocking {
        val querying = productRoom()
        val refused = stager.enqueueText(room, "refused", replyTo = null, request = false)
        val next = stager.enqueueText(room, "next", replyTo = null, request = false)
        coEvery { querying.userAction(room, match { (it as? ChatUserActionModel.NewMessage)?.messageId == refused.id }) } returns
            Result.failure(ProductChatQueryException.Rejected("no"))
        coEvery { querying.userAction(room, match { (it as? ChatUserActionModel.NewMessage)?.messageId == next.id }) } returns
            Result.success(ChatUserActionOutcome.Acknowledged)

        assertTrue(stager.processRoom(room))

        assertEquals(ProductAttachmentFailure.Refused("no"), outbox.get(refused.id)!!.failure)
        assertNull(outbox.get(next.id))
    }

    @Test
    fun `a message that the product keeps answering with an error fails after a few tries, so the next one goes`() = runBlocking {
        val querying = productRoom()
        val broken = stager.enqueueText(room, "broken", replyTo = null, request = false)
        val next = stager.enqueueText(room, "next", replyTo = null, request = false)
        coEvery { querying.userAction(room, match { (it as? ChatUserActionModel.NewMessage)?.messageId == broken.id }) } returns
            Result.failure(ProductChatQueryException.Failed("invite lost"))
        coEvery { querying.userAction(room, match { (it as? ChatUserActionModel.NewMessage)?.messageId == next.id }) } returns
            Result.success(ChatUserActionOutcome.Acknowledged)

        repeat(4) { assertFalse(stager.processRoom(room)) }
        assertNull(outbox.get(broken.id)!!.failure)
        assertTrue(stager.processRoom(room))

        assertTrue(outbox.get(broken.id)!!.isFailed)
        assertNull(outbox.get(next.id))
    }

    @Test
    fun `an edit and a reaction on a pending message go right after it`() = runBlocking {
        val querying = productRoom()
        val entry = stager.enqueueText(room, "helo", replyTo = null, request = false)
        assertTrue(stager.editPending(entry.id, "hello", at = 50))
        assertTrue(stager.toggleReactionPending(entry.id, "👍"))
        assertFalse(stager.editPending("unknown", "x", at = 1))
        coEvery { querying.userAction(room, any()) } answers {
            when (val action = secondArg<ChatUserActionModel>()) {
                is ChatUserActionModel.NewMessage ->
                    Result.success(ChatUserActionOutcome.MessageCreated(productMessage(action.messageId!!, text = "helo")))
                else -> Result.success(ChatUserActionOutcome.Acknowledged)
            }
        }

        assertTrue(stager.processRoom(room))

        coVerifyOrder {
            querying.userAction(room, match { it.isText("helo", replyTo = null, messageId = entry.id) })
            querying.userAction(
                room,
                match { it is ChatUserActionModel.EditMessage && it.messageId == entry.id && (it.content as ChatMessage.Content.Text).text == "hello" },
            )
            querying.userAction(room, ChatUserActionModel.ReactionPlaced(entry.id, "👍"))
        }
        // The room shows the message as the user left it, before the product confirms the edit and the reaction.
        coVerify {
            feedListeners.deliver(
                room,
                match { update ->
                    val message = (update as ProductFeedUpdateModel.New).message
                    (message.content as ChatMessage.Content.Text).text == "hello" && message.editedAt == 50L &&
                        message.reactions.single().reactedByUser
                },
            )
        }
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a reaction placed twice on a pending message is removed`() = runBlocking {
        val entry = stager.enqueueText(room, "hi", replyTo = null, request = false)

        stager.toggleReactionPending(entry.id, "👍")
        stager.toggleReactionPending(entry.id, "👍")

        assertNull(outbox.get(entry.id)!!.reaction)
    }

    @Test
    fun `delete removes the kept bytes`() = runBlocking {
        val entry = stager.enqueue(room, draft(), text = null, replyTo = null)

        assertTrue(stager.delete(entry.id))
        assertTrue(stager.processRoom(room))

        assertNull(outbox.get(entry.id))
        assertNull(outbox.bytes(entry.id))
        assertTrue(staging.hopFiles.isEmpty())
        assertFalse(stager.delete(entry.id))
    }

    @Test
    fun `the window shows each entry at its time with the native pending state`() = runBlocking {
        clock = 100
        val text = stager.enqueueText(room, "welcome", replyTo = null, request = true)
        clock = 300
        val uploading = stager.enqueue(room, draft(), text = "caption", replyTo = null)
        clock = 400
        val refused = stager.enqueueText(room, "refused", replyTo = null, request = false)
        outbox.update(refused.id) { it.copy(failure = ProductAttachmentFailure.Refused(null)) }
        stager.editPending(text.id, "welcome!", at = 150)
        stager.toggleReactionPending(text.id, "🔥")
        val window = ChatFeedSnapshot(
            messages = listOf(windowMessage("w2", 350), windowMessage("w1", 200)),
            reactions = emptyMap(),
            revisions = emptyMap(),
            productStatuses = emptyMap(),
            paging = ChatPagingState.Complete,
            readiness = ChatRoomReadiness.READY,
        )

        val shown = window.withOutbox(stager.observe(room).first(), stager::preview)

        assertEquals(listOf(refused.id, "w2", uploading.id, "w1", text.id), shown.messages.map { it.id })
        val byId = shown.messages.associateBy { it.id }
        assertEquals(ChatMessage.Status.NEW, byId.getValue(text.id).status)
        assertEquals(ChatMessage.Status.PROCESSING, byId.getValue(uploading.id).status)
        assertEquals(ChatMessage.Status.DELIVERY_FAILED, byId.getValue(refused.id).status)
        assertEquals(ProductMessageStatusModel.PREPARING, shown.productStatuses[text.id])
        assertEquals(ProductMessageStatusModel.FAILED, shown.productStatuses[refused.id])
        assertTrue(byId.getValue(text.id).content is ChatMessage.Content.ChatRequest)
        assertEquals(150L, shown.revisions.getValue(text.id).timestamp)
        assertEquals("🔥", shown.reactions.getValue(text.id).single().content.emoji)
        val preview = ((byId.getValue(uploading.id).content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText)
            .media.single().resource
        assertTrue(stager.isOutboxPreview(preview))
    }

    @Test
    fun `the window shows the message of the product once it has the id of an entry`() = runBlocking {
        val entry = stager.enqueueText(room, "hi", replyTo = null, request = false)
        val window = ChatFeedSnapshot(
            messages = listOf(windowMessage(entry.id, 5)),
            reactions = emptyMap(),
            revisions = emptyMap(),
            productStatuses = emptyMap(),
            paging = ChatPagingState.Complete,
            readiness = ChatRoomReadiness.READY,
        )

        assertSame(window, window.withOutbox(stager.observe(room).first(), stager::preview))
    }

    private fun draft(kind: ProductAttachmentDraft.Kind = ProductAttachmentDraft.Kind.IMAGE) = ProductAttachmentDraft(
        kind = kind,
        bytes = bytes,
        meta = ChatMediaMetaModel(mimeType = "image/jpeg", sizeBytes = bytes.size.toLong(), width = 4_000, height = 3_000, placeholder = null, alt = null),
        fileName = if (kind == ProductAttachmentDraft.Kind.FILE) "report.pdf" else null,
    )

    private fun ChatUserActionModel.isText(text: String, replyTo: String?, messageId: String) = this is ChatUserActionModel.NewMessage &&
        (content as? ChatMessage.Content.Text)?.text == text && this.replyTo == replyTo && this.messageId == messageId

    private fun productRoom(): ProductChatQuerying {
        val querying = mockk<ProductChatQuerying>()
        rooms.policy = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)
        rooms.querying = querying
        return querying
    }

    private fun windowMessage(id: String, timestamp: Long) = ChatMessage(
        id = id,
        chatId = room,
        timestamp = timestamp,
        origin = ChatMessageOrigin.Extension("ProductBot_chat.dot"),
        content = ChatMessage.Content.Text(id),
        status = ChatMessage.Status.IS_READ,
    )

    private fun productMessage(id: String, text: String = "x") = ProductRoomMessage(
        messageId = id,
        participantId = ProductParticipant.USER,
        timestamp = 1L,
        content = ChatMessage.Content.Text(text),
        status = ProductMessageStatusModel.SENT,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
        request = false,
    )

    private inner class FakeStaging : ProductAttachmentStaging {
        var result: Result<ChatResourceModel.Hop> = Result.success(hopFile)
        val hopFiles = mutableListOf<ByteArray>()
        var holds = 0

        override suspend fun <T> holdingWorker(chatId: ChatId, block: suspend () -> T): T {
            holds++
            return block()
        }

        /** What the stager showed of the upload while the node took the file. */
        var uploadsDuringUpload: Map<ChatMessageId, ProductAttachmentStager.UploadProgress>? = null

        override suspend fun uploadToHop(
            chatId: ChatId,
            bytes: ByteArray,
            progress: (sent: Long, total: Long) -> Unit,
        ): Result<ChatResourceModel.Hop> {
            hopFiles += bytes
            progress(bytes.size / 2L, bytes.size.toLong())
            uploadsDuringUpload = stager.observeUploads().first()
            return result
        }
    }

    private class FakeScheduler : ProductAttachmentStageScheduler {
        val scheduled = mutableListOf<ChatId>()

        override suspend fun schedule(chatId: ChatId) {
            scheduled += chatId
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
