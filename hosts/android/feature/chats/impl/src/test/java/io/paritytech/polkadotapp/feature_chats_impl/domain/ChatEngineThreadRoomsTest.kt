package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatThreadRooms
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.LastMessageSummary
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.coroutines.EmptyCoroutineContext

class ChatEngineThreadRoomsTest {
    private val lobby = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")
    private val thread = ChatId.forExtensionRoom("ProductBot_game.dot", "thread-lobby-m1")

    private val chatMessageRepository: ChatMessageRepository = mockk()
    private val chatExtensionRegistry: ChatExtensionRegistry = mockk()
    private val threadRoomIds = MutableStateFlow(emptySet<ChatId>())
    private val chatThreadRooms = ChatThreadRooms { threadRoomIds }

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
        productRoomAdditions = mockk(),
        chatThreadRooms = chatThreadRooms,
    )

    @Test
    fun `the chat list does not show a room that is the thread of a message`() = runBlocking<Unit> {
        withRooms(listOf(lobby, thread))
        threadRoomIds.value = setOf(thread)

        assertEquals(listOf(lobby), listedRooms())
    }

    @Test
    fun `a room shows in the chat list again when its features no longer name a parent`() = runBlocking<Unit> {
        withRooms(listOf(lobby, thread))
        threadRoomIds.value = emptySet()

        assertEquals(setOf(lobby, thread), listedRooms().toSet())
    }

    private fun withRooms(chatIds: List<ChatId>) {
        every { chatExtensionRegistry.observeActiveExtensions() } returns flowOf(emptyList())
        every { chatExtensionRegistry.observeAllExtensions() } returns flowOf(emptyList())
        every { chatMessageRepository.subscribeLastMessageSummaries(any()) } returns flowOf(chatIds.map(::emptyRoomSummary))
    }

    private suspend fun listedRooms(): List<ChatId> {
        val scope = object : ComputationalScope, CoroutineScope by CoroutineScope(EmptyCoroutineContext) {}
        return with(scope) { chatEngine.subscribeChatSummaries().first() }.map { it.chatId }
    }

    private fun emptyRoomSummary(chatId: ChatId) = LastMessageSummary(
        chatId = chatId,
        lastMessage = null,
        lastMessageSortOrder = null,
        unseenCount = 0,
        hasUnseenReaction = false,
        chatCreatedAt = 1,
        roomMetadata = RoomMetadata(name = null, icon = null),
    )
}
