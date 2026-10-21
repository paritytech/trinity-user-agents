package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import android.content.Context
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateService
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.DraftPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraftRelation
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallListPreviews
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
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

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
    private val userStateService = mockk<ChatRoomUserStateService>(relaxed = true)
    private val calls = RealChatRoomCallRepository()
    private val context = mockk<Context> {
        every { getString(RCommon.string.product_room_call_incoming) } returns "Incoming call"
        every { getString(RCommon.string.product_room_call_incoming_video) } returns "Incoming video call"
    }

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
    fun `a ringing call shows Incoming call as an ongoing preview until it stops to ring`() = runBlocking<Unit> {
        chats.value = listOf(productRoom(productText = "Rain at noon", ongoing = false))
        val viewModel = viewModel()

        calls.set(roomId, ChatRoomCallModel.Ringing("c1", video = true, caller = "p1"))
        val ringing = viewModel.loaded { (it.chats.firstOrNull()?.preview as? LastMessageUiModel.Text)?.message == "Incoming video call" }
        assertTrue(ringing.chats.first().isOngoing)

        calls.set(roomId, ChatRoomCallModel.Active("c1", video = true, startedAt = 1, mediaSession = null))
        val active = viewModel.loaded { (it.chats.firstOrNull()?.preview as? LastMessageUiModel.Text)?.message == "Rain at noon" }
        assertFalse(active.chats.first().isOngoing)
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

    @Test
    fun `a muted product room shows the mute`() = runBlocking<Unit> {
        val muted = ChatRoomUserStateModel(mutedUntil = ChatRoomUserStateModel.MUTED_WITHOUT_END, archived = false)
        chats.value = listOf(productRoom(productText = null, ongoing = false, userState = muted))

        val row = viewModel().firstRow()

        assertTrue(row.isMuted)
        assertEquals(ChatListUiState.UserState(isMuted = true, isArchived = false), row.userState)
    }

    @Test
    fun `an archived room leaves the main list for the archived section`() = runBlocking<Unit> {
        val archived = ChatRoomUserStateModel(mutedUntil = null, archived = true)
        chats.value = listOf(productRoom(productText = null, ongoing = false, userState = archived))
        val viewModel = viewModel()

        val main = viewModel.loaded { it.archivedCount == 1 }
        assertTrue(main.chats.isEmpty())
        assertTrue(main.hasArchivedRow)

        viewModel.onArchivedRowClick()
        val section = viewModel.loaded { it.showsArchived }
        assertEquals(listOf(roomId), section.chats.map { it.chatId })
        assertFalse(section.hasArchivedRow)

        viewModel.onArchivedBack()
        assertTrue(viewModel.loaded { !it.showsArchived }.chats.isEmpty())
    }

    @Test
    fun `the swipe actions unmute at once, ask the mute duration, and toggle the archive`() = runBlocking<Unit> {
        chats.value = listOf(productRoom(productText = null, ongoing = false))
        val viewModel = viewModel()
        val row = viewModel.firstRow()

        viewModel.onMuteToggle(row)
        assertEquals(roomId, viewModel.mutePicker.value)
        viewModel.onMuteDurationSelected(ChatMuteDuration.EIGHT_HOURS)
        assertNull(viewModel.mutePicker.value)
        coVerify { userStateService.mute(roomId, ChatMuteDuration.EIGHT_HOURS) }

        // The list decides from the row as it shows it now, not from the row that the gesture started on.
        val muted = ChatRoomUserStateModel(mutedUntil = ChatRoomUserStateModel.MUTED_WITHOUT_END, archived = false)
        chats.value = listOf(productRoom(productText = null, ongoing = false, userState = muted))
        viewModel.loaded { it.chats.single().isMuted }
        viewModel.onMuteToggle(row)
        coVerify { userStateService.unmute(roomId) }

        viewModel.onArchiveToggle(row)
        coVerify { userStateService.setArchived(roomId, archived = true) }
    }

    private suspend fun ChatListViewModel.loaded(predicate: (ChatListUiState) -> Boolean): ChatListUiState = withTimeout(5_000) {
        (state.first { it is LoadingState.Loaded && predicate(it.data) } as LoadingState.Loaded).data
    }

    private fun viewModel() = ChatListViewModel(
        interactor = interactor,
        router = mockk<ChatsRouter>(),
        messageMappingHelper = mockk(),
        roomPolicyMapper = ChatListRoomPolicyMapper(resolver),
        userStateService = userStateService,
        callPreviews = ProductRoomCallListPreviews(context, calls),
    )

    private suspend fun ChatListViewModel.firstRow(): ChatListUiState.ChatItem = withTimeout(5_000) {
        val loaded = state.first { it is LoadingState.Loaded && it.data.chats.isNotEmpty() }
        (loaded as LoadingState.Loaded).data.chats.first()
    }

    private fun productRoom(productText: String?, ongoing: Boolean, userState: ChatRoomUserStateModel = ChatRoomUserStateModel.NONE): Chat {
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
            roomPolicy = ChatRoomPolicy.Defaults.copy(listPreview = listPreview, userState = userState),
        )
    }
}
