package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.DefaultRoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatEngineProductRoomTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")

    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val extension: ChatExtension = mockk(relaxed = true)
    private val chatExtensionRegistry: ChatExtensionRegistry = mockk()
    private val chatRoomRepository: ChatRoomRepository = mockk()
    private val contactRooms = MutableStateFlow<ChatRoomDeclarations?>(null)
    private val declarationsRepository = mockk<ChatRoomDeclarationsRepository> {
        every { observe(room) } returns contactRooms
        coEvery { get(room) } answers { contactRooms.value }
    }

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
        productRoomAdditions = mockk(),
        declarationsRepository = declarationsRepository,
    )

    @Test
    fun `the header of a room without a contact reads its name once, as before the chat product`() = runBlocking<Unit> {
        withExtension()
        every { extension.defaultRoomMetadata } returns DefaultRoomMetadata("Game", icon = null)
        coEvery { chatRoomRepository.getRoomMetadata(room) } returns RoomMetadata(name = "Lobby", icon = null)

        val name = chatEngine.observeChatDisplay(room).first().getOrThrow().name

        assertEquals("Lobby", name)
        coVerify(exactly = 1) { chatRoomRepository.getRoomMetadata(room) }
    }

    @Test
    fun `a room of the chat product shows its contact as a person chat shows it`() = runBlocking<Unit> {
        withExtension()
        every { extension.defaultRoomMetadata } returns DefaultRoomMetadata("Chats", icon = null)
        val account = ByteArray(32) { 3 }.toDataByteArray()
        contactRooms.value = ChatRoomDeclarations.EMPTY.copy(
            contact = ChatRoomContactModel(account, username = "alice", state = ChatContactStateModel.ESTABLISHED),
        )

        val display = chatEngine.observeChatDisplay(room).first().getOrThrow()

        assertEquals("alice", display.name)
        val avatar = display.avatar as ChatAvatar.Account
        assertEquals("alice", avatar.name)
        assertArrayEquals(account.value, avatar.themeSeed)
        assertEquals("alice", chatEngine.getChatDisplay(room).getOrThrow().name)
    }

    private fun withExtension() {
        coEvery { chatExtensionRegistry.getExtensionForChat(any()) } returns extension
    }
}
