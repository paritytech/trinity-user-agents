package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MessageRevisionRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ProductFeedApplierTest {
    private val extensionId = "ProductBot_game.dot"
    private val room = ChatId.forExtensionRoom(extensionId, "lobby")
    private val otherRoom = ChatId.forExtensionRoom(extensionId, "hall")
    private val decoder: CustomContentDecoder = mockk()

    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val messageRevisionRepository: MessageRevisionRepository = mockk(relaxed = true)
    private val chatRoomRepository: ChatRoomRepository = mockk()

    private val applier = ProductFeedApplier(chatMessageRepository, messageRevisionRepository, chatRoomRepository)
    private val saved = mutableListOf<ChatMessage>()

    @Before
    fun setUp() {
        coEvery { chatRoomRepository.roomExists(room) } returns true
        coEvery { chatMessageRepository.getMessageById(any(), any(), any()) } returns null
    }

    @Test
    fun `a new message is stored under an id of its room`() = runBlocking<Unit> {
        val result = apply(ProductFeedUpdateModel.New(productMessage("m1", participant = "alice")))

        assertSuccess(result)
        val message = saved.single()
        assertEquals(ProductMessageIds.stored("m1", room), message.id)
        assertEquals(ChatMessage.Status.NEW, message.status)
        assertTrue(message.origin is ChatMessageOrigin.Extension)
    }

    @Test
    fun `a new message that names the user is refused`() = runBlocking<Unit> {
        assertRejected<ProductFeedRejection.NotOwner>(apply(ProductFeedUpdateModel.New(productMessage("m1", participant = "user"))))
        assertTrue(saved.isEmpty())
    }

    @Test
    fun `a new message with an id the room holds is a duplicate`() = runBlocking<Unit> {
        withStoredMessage(productStored("m1"))

        assertRejected<ProductFeedRejection.DuplicateMessage>(apply(ProductFeedUpdateModel.New(productMessage("m1"))))
    }

    @Test
    fun `a message of another room with the same id is not this room's message`() = runBlocking<Unit> {
        withStoredMessage(message(id = "m1", chatId = otherRoom, origin = ChatMessageOrigin.User))

        assertRejected<ProductFeedRejection.UnknownMessage>(apply(ProductFeedUpdateModel.Replaced(productMessage("m1"))))
    }

    @Test
    fun `an edit replaces the body in place and keeps one revision`() = runBlocking<Unit> {
        val stored = productStored("m1")
        withStoredMessage(stored)
        val body = ChatMessage.Content.Text("longer answer")

        assertSuccess(apply(ProductFeedUpdateModel.Edited("m1", body, editedAt = 7)))

        coVerifyOrder {
            chatMessageRepository.updateMessageContent(stored.id, body, decoder)
            messageRevisionRepository.deleteRevisions(stored.id)
            messageRevisionRepository.saveRevision(stored.id, body, room, 7, decoder)
        }
    }

    @Test
    fun `an edit of a message of the user is refused`() = runBlocking<Unit> {
        withStoredMessage(message(id = "u1", chatId = room, origin = ChatMessageOrigin.User))

        assertRejected<ProductFeedRejection.NotOwner>(apply(ProductFeedUpdateModel.Edited("u1", ChatMessage.Content.Text("x"), 1)))
    }

    @Test
    fun `an edit of a message that is not there is refused`() = runBlocking<Unit> {
        assertRejected<ProductFeedRejection.UnknownMessage>(apply(ProductFeedUpdateModel.Edited("m9", ChatMessage.Content.Text("x"), 1)))
    }

    @Test
    fun `a replaced message takes the new body without an edit mark`() = runBlocking<Unit> {
        val stored = productStored("m1")
        withStoredMessage(stored)

        assertSuccess(apply(ProductFeedUpdateModel.Replaced(productMessage("m1"))))

        coVerify { chatMessageRepository.updateMessageContent(stored.id, match { (it as ChatMessage.Content.Text).text == "hi" }, decoder) }
        coVerify(exactly = 0) { messageRevisionRepository.saveRevision(any(), any(), any(), any(), any()) }
    }

    @Test
    fun `a replacement of a message of the user is refused`() = runBlocking<Unit> {
        withStoredMessage(message(id = "u1", chatId = room, origin = ChatMessageOrigin.User))

        assertRejected<ProductFeedRejection.NotOwner>(apply(ProductFeedUpdateModel.Replaced(productMessage("u1"))))
    }

    @Test
    fun `a reaction of a participant is stored as a reaction of the product`() = runBlocking<Unit> {
        val stored = productStored("m1")
        withStoredMessage(stored)
        val reaction = slot<ChatMessageReaction>()
        coEvery { chatMessageRepository.addReaction(capture(reaction), room) } returns Unit

        assertSuccess(apply(ProductFeedUpdateModel.ReactionPlaced("m1", "alice", "🔥")))

        assertEquals(stored.id, reaction.captured.messageId)
        assertEquals("🔥", reaction.captured.content.emoji)
        assertTrue(reaction.captured.origin is ChatMessageOrigin.Extension)
    }

    @Test
    fun `a reaction in the name of the user is refused`() = runBlocking<Unit> {
        withStoredMessage(productStored("m1"))

        assertRejected<ProductFeedRejection.NotOwner>(apply(ProductFeedUpdateModel.ReactionRemoved("m1", "user", "🔥")))
    }

    @Test
    fun `a status change of a known message is accepted and stores nothing`() = runBlocking<Unit> {
        withStoredMessage(productStored("m1"))

        assertSuccess(apply(ProductFeedUpdateModel.StatusChanged("m1", ProductMessageStatusModel.READ)))

        coVerify(exactly = 0) { chatMessageRepository.updateMessageStatus(any(), any()) }
    }

    // As on iOS: the product tells the user when it sent the message on, when the other side has it, and a failure.
    @Test
    fun `a status change of a message of the user is the status that the user sees`() = runBlocking<Unit> {
        withStoredMessage(message("u1", room, ChatMessageOrigin.User))

        for ((status, stored) in listOf(
            ProductMessageStatusModel.SENT to ChatMessage.Status.IS_SENT,
            ProductMessageStatusModel.DELIVERED to ChatMessage.Status.IS_READ,
            ProductMessageStatusModel.READ to ChatMessage.Status.IS_READ,
            ProductMessageStatusModel.FAILED to ChatMessage.Status.DELIVERY_FAILED,
            ProductMessageStatusModel.PREPARING to ChatMessage.Status.NEW,
        )) {
            assertSuccess(apply(ProductFeedUpdateModel.StatusChanged("u1", status)))
            coVerify { chatMessageRepository.updateMessageStatus("u1", stored) }
        }
    }

    @Test
    fun `an update for a room that does not exist is an unknown room`() = runBlocking<Unit> {
        coEvery { chatRoomRepository.roomExists(room) } returns false

        assertRejected<ProductFeedRejection.UnknownRoom>(apply(ProductFeedUpdateModel.Replaced(productMessage("m1"))))
    }

    private suspend fun apply(update: ProductFeedUpdateModel) = applier.apply(extensionId, room, update, decoder) { saved += it }

    private fun withStoredMessage(message: ChatMessage) {
        coEvery { chatMessageRepository.getMessageById(any(), message.id, any()) } returns message
    }

    private fun productMessage(id: String, participant: String = "product") = ProductRoomMessage(
        messageId = id,
        participantId = participant,
        timestamp = 5,
        content = ChatMessage.Content.Text("hi"),
        status = ProductMessageStatusModel.SENT,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
        request = false,
    )

    private fun productStored(id: String, content: ChatMessage.Content = ChatMessage.Content.Text("hi")) =
        message(ProductMessageIds.stored(id, room), room, ChatMessageOrigin.Extension(extensionId), content)

    private fun message(
        id: String,
        chatId: ChatId,
        origin: ChatMessageOrigin,
        content: ChatMessage.Content = ChatMessage.Content.Text("hi"),
    ) = ChatMessage(id = id, chatId = chatId, timestamp = 1, origin = origin, content = content, status = ChatMessage.Status.IS_READ)

    private fun assertSuccess(result: Result<Unit>) {
        assertTrue("expected success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    private inline fun <reified T : ProductFeedRejection> assertRejected(result: Result<Unit>) {
        assertTrue("expected ${T::class.simpleName} but was ${result.exceptionOrNull()}", result.exceptionOrNull() is T)
    }
}
