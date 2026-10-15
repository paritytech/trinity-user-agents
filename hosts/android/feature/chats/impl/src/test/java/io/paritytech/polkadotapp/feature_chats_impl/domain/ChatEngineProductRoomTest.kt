package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.DefaultRoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatEngineProductRoomTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")

    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val extension: ChatExtension = mockk(relaxed = true)
    private val chatExtensionRegistry: ChatExtensionRegistry = mockk()
    private val chatRoomRepository: ChatRoomRepository = mockk()

    private val chatEngine = ChatEngine(
        chatMessageRepository = chatMessageRepository,
        chatMessageProcessingRepository = mockk(),
        chatExtensionRegistry = chatExtensionRegistry,
        accountRepository = mockk(),
        contactsRepository = mockk(),
        messageRevisionRepository = mockk(),
        dmChatMessageOriginDisplayResolverFactory = mockk(),
        chatDisplayGenerator = ChatDisplayGenerator(mockk()),
        messageSaveProcessors = emptySet(),
        headerRenderers = emptyMap(),
        originConfigurations = emptyMap(),
        chatRoomRepository = chatRoomRepository,
        deleteRoomUseCase = mockk(),
        productRoomDeclarer = mockk(),
        productFeedApplier = mockk(),
    )

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

    @Test
    fun `the open room header follows a new name of the room`() = runBlocking<Unit> {
        withExtension()
        every { extension.defaultRoomMetadata } returns DefaultRoomMetadata("Game", icon = null)
        every { chatRoomRepository.subscribeRoomMetadata(room) } returns
            flowOf(RoomMetadata(name = "Lobby", icon = null), RoomMetadata(name = "Renamed", icon = null))

        val names = chatEngine.observeChatDisplay(room).map { it.getOrThrow().name }.toList()

        assertEquals(listOf("Lobby", "Renamed"), names)
    }

    private fun withExtension() {
        coEvery { chatExtensionRegistry.getExtensionForChat(any()) } returns extension
    }
}
