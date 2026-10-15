package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBranchPositionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatReasoningModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatThreadParentModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatMessageMetaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CustomContentDecoder
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductRoomAdditionsTest {
    private val extensionId = "ProductBot_game.dot"
    private val room = ChatId.forExtensionRoom(extensionId, "lobby")
    private val decoder: CustomContentDecoder = mockk()
    private val meta = ChatMessageMetaModel(thread = null, branch = ChatBranchPositionModel(1, 2), reasoning = null)

    private val chatRoomRepository: ChatRoomRepository = mockk()
    private val chatMessageRepository: ChatMessageRepository = mockk()
    private val declarationsRepository: ChatRoomDeclarationsRepository = mockk()
    private val declarationsWriter: ChatRoomDeclarationsWriter = mockk(relaxed = true)
    private val metaWriter: ChatMessageMetaWriter = mockk(relaxed = true)
    private val callWriter: ChatRoomCallWriter = mockk(relaxed = true)

    private val additions = ProductRoomAdditions(
        chatRoomRepository = chatRoomRepository,
        chatMessageRepository = chatMessageRepository,
        declarationsRepository = declarationsRepository,
        declarationsWriter = declarationsWriter,
        metaWriter = metaWriter,
        callWriter = callWriter,
    )

    @Test
    fun `features of an own room are kept whole, also those that the host does not serve`() = runBlocking<Unit> {
        withRoomExisting(true)
        val features = ChatRoomFeaturesModel.HOST_DEFAULT.copy(threadOf = ChatThreadParentModel("lobby", "m1"), readAloud = true)

        val result = additions.setFeatures(extensionId, room, features)

        assertTrue(result.isSuccess)
        coVerify { declarationsWriter.setFeatures(room, features) }
    }

    @Test
    fun `features for the room of another extension fail as an unknown room`() = runBlocking<Unit> {
        withRoomExisting(true)

        val result = additions.setFeatures("ProductBot_other.dot", room, ChatRoomFeaturesModel.HOST_DEFAULT)

        assertTrue("expected UnknownChatRoomException but was $result", result.exceptionOrNull() is UnknownChatRoomException)
        coVerify(exactly = 0) { declarationsWriter.setFeatures(any(), any()) }
    }

    @Test
    fun `meta of a stored message is kept with the time of the message`() = runBlocking<Unit> {
        withRoomExisting(true)
        withProductSource(false)
        withStoredMessage("m1", timestamp = 42)

        additions.setMessageMeta(extensionId, room, "m1", meta, decoder)

        coVerify { metaWriter.put(room, "m1", 42, meta, keepNewest = null) }
    }

    @Test
    fun `a stored reasoning message is marked as read, so it is not counted as unread`() = runBlocking<Unit> {
        withRoomExisting(true)
        withProductSource(false)
        withStoredMessage("m1", timestamp = 42)
        coEvery { chatMessageRepository.markMessageAsRead(any()) } returns Unit

        additions.setMessageMeta(extensionId, room, "m1", ChatMessageMetaModel(null, null, ChatReasoningModel(durationMs = null)), decoder)

        coVerify { chatMessageRepository.markMessageAsRead(ProductMessageIds.stored("m1", room)) }
    }

    @Test
    fun `meta of a message that a host-store room does not hold fails as an unknown message`() = runBlocking<Unit> {
        withRoomExisting(true)
        withProductSource(false)
        coEvery { chatMessageRepository.getMessageById(room, any(), decoder) } returns null

        val result = additions.setMessageMeta(extensionId, room, "missing", meta, decoder)

        assertTrue("expected UnknownMessage but was $result", result.exceptionOrNull() is ProductFeedRejection.UnknownMessage)
        coVerify(exactly = 0) { metaWriter.put(any(), any(), any(), any(), any()) }
    }

    @Test
    fun `meta in a product-source room is kept without a lookup, for the newest messages only`() = runBlocking<Unit> {
        withRoomExisting(true)
        withProductSource(true)

        additions.setMessageMeta(extensionId, room, "not-in-any-window", meta, decoder)

        coVerify { metaWriter.put(room, "not-in-any-window", any(), meta, keepNewest = PRODUCT_SOURCE_META_KEPT) }
        coVerify(exactly = 0) { chatMessageRepository.getMessageById(any(), any(), any()) }
    }

    @Test
    fun `an empty meta removes the meta of the message`() = runBlocking<Unit> {
        withRoomExisting(true)
        withProductSource(true)

        additions.setMessageMeta(extensionId, room, "m1", ChatMessageMetaModel(null, null, null), decoder)

        coVerify { metaWriter.delete(room, "m1") }
    }

    @Test
    fun `a call is kept in memory and removed with null`() = runBlocking<Unit> {
        withRoomExisting(true)
        val call = ChatRoomCallModel.Ringing("c1", video = false, caller = "p1")

        additions.setCall(extensionId, room, call)
        additions.setCall(extensionId, room, null)

        verify { callWriter.set(room, call) }
        verify { callWriter.set(room, null) }
    }

    private fun withRoomExisting(exists: Boolean) {
        coEvery { chatRoomRepository.roomExists(room) } returns exists
    }

    private fun withProductSource(productSource: Boolean) {
        coEvery { declarationsRepository.get(room) } returns
            ChatRoomDeclarations.EMPTY.copy(config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = productSource))
    }

    private fun withStoredMessage(productMessageId: String, timestamp: Long) {
        val stored = ProductMessageIds.stored(productMessageId, room)
        val message = ChatMessage.new(messageId = stored, chatId = room, content = ChatMessage.Content.Text("hi"), origin = ChatMessageOrigin.Extension(extensionId))
            .copy(timestamp = timestamp)
        coEvery { chatMessageRepository.getMessageById(room, stored, decoder) } returns message
    }
}
