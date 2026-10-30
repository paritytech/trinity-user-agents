package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentFailure
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductAttachmentOutbox
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
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

    private val room = ChatId.forExtensionRoom("ProductBot_weather", "lobby")

    private var clock = 1_000L
    private val outbox by lazy { ProductAttachmentOutbox(folder.root, Dispatchers.Unconfined) { clock } }
    private val scheduler = FakeScheduler()
    private val rooms = FakeRooms()
    private val feedListeners = mockk<ProductFeedListeners>(relaxed = true)
    private val stager by lazy { ProductAttachmentStager(outbox, scheduler, rooms, feedListeners) }

    @Test
    fun `a text goes to the product under the id of its entry, and the room shows the message that the product made`() = runBlocking {
        val querying = productRoom()
        val entry = stager.enqueueText(room, "hi", replyTo = "m0")
        val created = productMessage(entry.id)
        coEvery { querying.userAction(room, any()) } returns Result.success(ChatUserActionOutcome.MessageCreated(created))

        assertTrue(stager.processRoom(room))

        coVerify { querying.userAction(room, match { it.isText("hi", replyTo = "m0", messageId = entry.id) }) }
        coVerify { feedListeners.deliver(room, ProductFeedUpdateModel.New(created)) }
        assertEquals(listOf(room), scheduler.scheduled)
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `NewMessage carries the time the user wrote the message`() = runBlocking {
        val querying = productRoom()
        clock = 4_242L
        val entry = stager.enqueueText(room, "hi", replyTo = null)
        clock = 9_000L
        coEvery { querying.userAction(room, any()) } returns Result.success(ChatUserActionOutcome.MessageCreated(productMessage(entry.id)))

        assertTrue(stager.processRoom(room))

        coVerify { querying.userAction(room, match { (it as ChatUserActionModel.NewMessage).timestamp == 4_242L }) }
    }

    @Test
    fun `a room that the host database keeps now stores the message`() = runBlocking {
        rooms.policy = ChatRoomPolicy.Defaults
        val entry = stager.enqueueText(room, "hi", replyTo = "m0")

        assertTrue(stager.processRoom(room))

        val (chatId, content, replyTo) = rooms.saved.single()
        assertEquals(room, chatId)
        assertEquals("hi", (content as ChatMessage.Content.Text).text)
        assertEquals("m0", replyTo)
        assertNull(outbox.get(entry.id))
    }

    @Test
    fun `a message that did not go goes again under the same id, before the messages after it`() = runBlocking {
        val querying = productRoom()
        val first = stager.enqueueText(room, "first", replyTo = null)
        val second = stager.enqueueText(room, "second", replyTo = first.id)
        coEvery { querying.userAction(room, any()) } returns Result.failure(ProductChatQueryException.NoAnswer())

        assertFalse(stager.processRoom(room))

        // The message stays pending, never failed, while it cannot go yet.
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
    fun `a message that the product refuses fails, and the next message goes`() = runBlocking {
        val querying = productRoom()
        val refused = stager.enqueueText(room, "refused", replyTo = null)
        val next = stager.enqueueText(room, "next", replyTo = null)
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
        val broken = stager.enqueueText(room, "broken", replyTo = null)
        val next = stager.enqueueText(room, "next", replyTo = null)
        coEvery { querying.userAction(room, match { (it as? ChatUserActionModel.NewMessage)?.messageId == broken.id }) } returns
            Result.failure(ProductChatQueryException.Failed("lost"))
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
        val entry = stager.enqueueText(room, "helo", replyTo = null)
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
        val entry = stager.enqueueText(room, "hi", replyTo = null)

        stager.toggleReactionPending(entry.id, "👍")
        stager.toggleReactionPending(entry.id, "👍")

        assertNull(outbox.get(entry.id)!!.reaction)
    }

    @Test
    fun `opening a room sends again what waits, and nothing for a room with only failed messages`() = runBlocking {
        val entry = stager.enqueueText(room, "hi", replyTo = null)
        scheduler.scheduled.clear()

        stager.resume(room)
        outbox.update(entry.id) { it.copy(failure = ProductAttachmentFailure.Refused(null)) }
        stager.resume(room)

        assertEquals(listOf(room), scheduler.scheduled)
    }

    @Test
    fun `the window shows each entry at its time in the pending state`() = runBlocking {
        clock = 100
        val text = stager.enqueueText(room, "hello", replyTo = null)
        clock = 400
        val refused = stager.enqueueText(room, "refused", replyTo = null)
        outbox.update(refused.id) { it.copy(failure = ProductAttachmentFailure.Refused(null)) }
        stager.editPending(text.id, "hello!", at = 150)
        stager.toggleReactionPending(text.id, "🔥")
        val window = ChatFeedSnapshot(
            messages = listOf(windowMessage("w2", 350), windowMessage("w1", 200)),
            reactions = emptyMap(),
            revisions = emptyMap(),
            paging = ChatPagingState.Complete,
            readiness = ChatRoomReadiness.READY,
        )

        val shown = window.withOutbox(stager.observe(room).first())

        assertEquals(listOf(refused.id, "w2", "w1", text.id), shown.messages.map { it.id })
        val byId = shown.messages.associateBy { it.id }
        assertEquals(ChatMessage.Status.NEW, byId.getValue(text.id).status)
        assertEquals(ChatMessage.Status.DELIVERY_FAILED, byId.getValue(refused.id).status)
        assertSame(ChatMessageOrigin.User, byId.getValue(text.id).origin)
        assertEquals(150L, shown.revisions.getValue(text.id).timestamp)
        assertEquals("🔥", shown.reactions.getValue(text.id).single().content.emoji)
    }

    @Test
    fun `the window shows the message of the product once it has the id of an entry`() = runBlocking {
        val entry = stager.enqueueText(room, "hi", replyTo = null)
        val window = ChatFeedSnapshot(
            messages = listOf(windowMessage(entry.id, 5)),
            reactions = emptyMap(),
            revisions = emptyMap(),
            paging = ChatPagingState.Complete,
            readiness = ChatRoomReadiness.READY,
        )

        assertSame(window, window.withOutbox(stager.observe(room).first()))
    }

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
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
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

    private class FakeScheduler : ProductAttachmentStageScheduler {
        val scheduled = mutableListOf<ChatId>()

        override suspend fun schedule(chatId: ChatId) {
            scheduled += chatId
        }
    }

    private class FakeRooms : ProductAttachmentRooms {
        var policy: ChatRoomPolicy? = ChatRoomPolicy.Defaults
        var querying: ProductChatQuerying? = null
        val saved = mutableListOf<Triple<ChatId, ChatMessage.Content, ChatMessageId?>>()

        override suspend fun policy(chatId: ChatId): ChatRoomPolicy? = policy

        override suspend fun querying(chatId: ChatId): ProductChatQuerying? = querying

        override suspend fun saveUserMessage(chatId: ChatId, content: ChatMessage.Content, replyTo: ChatMessageId?) {
            saved += Triple(chatId, content, replyTo)
        }
    }
}
