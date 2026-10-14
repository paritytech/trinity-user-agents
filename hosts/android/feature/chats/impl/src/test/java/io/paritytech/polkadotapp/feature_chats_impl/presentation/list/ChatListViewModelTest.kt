package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.DraftPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraftRelation
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ChatListViewModelTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "berlin")

    private val chats = MutableStateFlow<List<Chat>>(emptyList())
    private val drafts = MutableStateFlow<Map<ChatId, ChatDraft>>(emptyMap())

    private val interactor = mockk<ChatListInteractor> {
        every { with(any<ComputationalScope>()) { subscribeChats() } } returns chats
        every { with(any<ComputationalScope>()) { subscribeCallSignaling() } } returns flowOf(emptyList())
        every { subscribeDrafts() } returns drafts
        every { subscribeAllCustomMessageRenderers() } returns flowOf(emptyMap())
        every { subscribePendingIncomingRequestsCount() } returns flowOf(0)
        every { subscribeActiveCall() } returns flowOf(null)
    }
    private val resolver = mockk<ChatResourceResolver>()

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `the product text replaces the last message of the row`() = runBlocking<Unit> {
        chats.value = listOf(productRoom(productText = "Rain at noon", ongoing = false))

        val row = viewModel().firstRow()

        assertEquals("Rain at noon", (row.preview as LastMessageUiModel.Text).message)
        assertFalse(row.isOngoing)
    }

    @Test
    fun `an ongoing product text marks the row`() = runBlocking<Unit> {
        chats.value = listOf(productRoom(productText = "Bot is typing", ongoing = true))

        assertTrue(viewModel().firstRow().isOngoing)
    }

    @Test
    fun `a draft wins over the product text and stops the activity`() = runBlocking<Unit> {
        chats.value = listOf(productRoom(productText = "Bot is typing", ongoing = true))
        drafts.value = mapOf(roomId to ChatDraft(text = "half a thought", relation = ChatDraftRelation.None))

        val row = viewModel().firstRow()

        assertEquals("half a thought", (row.preview as DraftPreviewUiModel).text)
        assertFalse(row.isOngoing)
    }

    @Test
    fun `a room without a product text shows its last message`() = runBlocking<Unit> {
        chats.value = listOf(productRoom(productText = null, ongoing = false))

        assertEquals("Sunny", (viewModel().firstRow().preview as LastMessageUiModel.Text).message)
    }

    private fun viewModel() = ChatListViewModel(
        interactor = interactor,
        router = mockk<ChatsRouter>(),
        messageMappingHelper = mockk(),
        roomPolicyMapper = ChatListRoomPolicyMapper(resolver),
    )

    private suspend fun ChatListViewModel.firstRow(): ChatListUiState.ChatItem = withTimeout(5_000) {
        val loaded = state.first { it is LoadingState.Loaded && it.data.chats.isNotEmpty() }
        (loaded as LoadingState.Loaded).data.chats.first()
    }

    private fun productRoom(productText: String?, ongoing: Boolean): Chat {
        val lastMessage = ChatMessage(
            id = "message-1",
            chatId = roomId,
            timestamp = 10L,
            origin = ChatMessageOrigin.Extension("ProductBot_weather"),
            content = ChatMessage.Content.Text("Sunny"),
            status = ChatMessage.Status.IS_READ,
        )
        val listPreview = productText?.let { ChatRoomPolicy.ListPreview.Text(it, ongoing) } ?: ChatRoomPolicy.ListPreview.LastMessage

        return Chat(
            id = roomId,
            display = ChatDisplay("Weather", ChatAvatar.Account("Weather", byteArrayOf(1))),
            preview = ChatPreview.Message(lastMessage),
            timestamp = 10L,
            unreadBadge = ChatSummaryBadge.None,
            hasUnseenReaction = false,
            customPreviewRenderer = null,
            roomPolicy = ChatRoomPolicy.Defaults.copy(listPreview = listPreview),
        )
    }
}
