package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatEngineProductRoomTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")
    private val contactRoom = ChatId.fromContact(ByteArray(32) { 1 }.intoAccountId())

    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val extension: ChatExtension = mockk(relaxed = true)
    private val chatExtensionRegistry: ChatExtensionRegistry = mockk()

    private val chatEngine = ChatEngine(
        chatMessageRepository = chatMessageRepository,
        chatMessageProcessingRepository = mockk(),
        chatExtensionRegistry = chatExtensionRegistry,
        accountRepository = mockk(),
        contactsRepository = mockk(),
        messageRevisionRepository = mockk(),
        dmChatMessageOriginDisplayResolverFactory = mockk(),
        chatDisplayGenerator = mockk(),
        messageSaveProcessors = emptySet(),
        headerRenderers = emptyMap(),
        originConfigurations = emptyMap(),
        chatRoomRepository = mockk(),
        deleteRoomUseCase = mockk(),
        productRoomDeclarer = mockk(),
        productFeedApplier = mockk(),
    )

    @Test
    fun `a user delete in a product room keeps a tombstone and tells the extension`() = runBlocking<Unit> {
        withExtension()
        withMessage("m1", room)

        val result = chatEngine.deleteMessageForUser(room, "m1")

        assertTrue("expected success but was ${result.exceptionOrNull()}", result.isSuccess)
        coVerify { chatMessageRepository.markMessageDeleted("m1") }
        coVerify { extension.onUserEvent(room, ChatRoomUserEvent.MessageDeleted("m1")) }
    }

    @Test
    fun `a person chat has no user delete`() = runBlocking<Unit> {
        val result = chatEngine.deleteMessageForUser(contactRoom, "m1")

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { chatMessageRepository.markMessageDeleted(any()) }
    }

    @Test
    fun `a message of another room is not deleted`() = runBlocking<Unit> {
        withExtension()
        withMessage("m1", ChatId.forExtensionRoom("ProductBot_game.dot", "hall"))

        assertTrue(chatEngine.deleteMessageForUser(room, "m1").isFailure)
        coVerify(exactly = 0) { chatMessageRepository.markMessageDeleted(any()) }
    }

    @Test
    fun `reading a product room reports the newest visible message to the extension`() = runBlocking<Unit> {
        withExtension()
        coEvery { chatMessageRepository.getNewestVisibleMessageIdUpTo(room, 42) } returns "m7"

        chatEngine.reportSeen(room, 42)

        coVerify { extension.onUserEvent(room, ChatRoomUserEvent.MessageSeen("m7")) }
    }

    @Test
    fun `a typed command goes to the extension by name and arguments`() = runBlocking<Unit> {
        withExtension()
        coEvery { extension.handleCommand(room, "roll", "2") } returns true

        assertTrue(chatEngine.handleCommand(room, "/roll 2"))
        assertFalse(chatEngine.handleCommand(room, "roll 2"))
    }

    private fun withExtension() {
        coEvery { chatExtensionRegistry.getExtensionForChat(any()) } returns extension
    }

    private fun withMessage(id: String, chatId: ChatId) {
        coEvery { chatMessageRepository.getMessageById(any(), id, any()) } returns ChatMessage(
            id = id,
            chatId = chatId,
            timestamp = 1,
            origin = ChatMessageOrigin.User,
            content = ChatMessage.Content.Text("hi"),
            status = ChatMessage.Status.IS_READ,
        )
    }
}
