package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPageOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMessagesState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatPageEdgeUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatRoomReadinessUiState
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

/** The paging, readiness and room lifecycle of the room screen of a product-source room. */
class ChatFeedViewModelPagingTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")
    private val feed = MutableStateFlow(snapshot(older = ChatPagingState.Edge.LOADING, readiness = ChatRoomReadiness.LOADING))

    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        every { subscribeMessages(roomId) } returns feed
        coEvery { getLastRevealedTimestamp(any()) } returns null
        coEvery { loadOlderMessages(any()) } returns ChatPageOutcome.Appended(0)
        coEvery { loadNewerMessages(any()) } returns ChatPageOutcome.Appended(0)
        coEvery { jumpToMessage(any(), any()) } returns false
        coEvery { isProductSource(roomId) } returns true
    }
    private val messageUiMapper = mockk<ChatMessageUiMapper> {
        coEvery { map(any(), any(), any(), any(), any(), any(), any()) } returns persistentListOf()
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() {
        // A view model that a test leaves open would run on the main dispatcher of the next test.
        models.forEach { it.viewModelScope.cancel() }
        Dispatchers.resetMain()
    }

    private val models = mutableListOf<ChatFeedViewModel>()

    @Test
    fun `the feed shows the paging and the readiness of the source`() = runBlocking<Unit> {
        val model = viewModel()
        feed.value = snapshot(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.LOADING, readiness = ChatRoomReadiness.UNAVAILABLE)

        val state = model.awaitMessagesState { it.readiness == ChatRoomReadinessUiState.UNAVAILABLE }

        assertEquals(ChatPageEdgeUiState.MORE, state.olderPage)
        assertEquals(ChatPageEdgeUiState.LOADING, state.newerPage)
    }

    @Test
    fun `the feed asks older messages only when more exist`() = runBlocking<Unit> {
        val model = viewModel()
        feed.value = snapshot(older = ChatPagingState.Edge.END, readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.olderPage == ChatPageEdgeUiState.END }

        model.onLoadOlderMessages()
        coVerify(exactly = 0) { interactor.loadOlderMessages(any()) }

        feed.value = snapshot(older = ChatPagingState.Edge.MORE, readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.olderPage == ChatPageEdgeUiState.MORE }

        model.onLoadOlderMessages()
        coVerify(timeout = 5_000) { interactor.loadOlderMessages(roomId) }
    }

    @Test
    fun `the newest button loads the newest page when the window is away from it`() = runBlocking<Unit> {
        val model = viewModel()
        feed.value = snapshot(newer = ChatPagingState.Edge.MORE, readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.newerPage == ChatPageEdgeUiState.MORE }

        model.onShowNewestMessages()

        coVerify(timeout = 5_000) { interactor.showNewestMessages(roomId) }
    }

    @Test
    fun `a tap on a quote of an unloaded message asks the source to jump`() = runBlocking<Unit> {
        val model = viewModel()
        feed.value = snapshot(readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.readiness == ChatRoomReadinessUiState.READY }

        model.onMessageAction(MessageAction.ReplyPreviewTap("m10"))

        coVerify(timeout = 5_000) { interactor.jumpToMessage(roomId, "m10") }
    }

    @Test
    fun `a jump in a room that the host keeps waits for the message`() = runBlocking<Unit> {
        coEvery { interactor.isProductSource(roomId) } returns false
        val model = viewModel()
        feed.value = snapshot(readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.readiness == ChatRoomReadinessUiState.READY }

        model.onMessageAction(MessageAction.ReplyPreviewTap("m10"))

        coVerify(timeout = 5_000) { interactor.isProductSource(roomId) }
        coVerify(exactly = 0) { interactor.jumpToMessage(any(), any()) }
    }

    @Test
    fun `the screen opens the room and sends its outbox on start, and closes it on stop`() = runBlocking<Unit> {
        val model = viewModel()

        model.onScreenStarted()
        model.onScreenStopped()

        verify { interactor.openRoom(roomId) }
        coVerify(timeout = 5_000) { interactor.resumeOutbox(roomId) }
        verify { interactor.closeRoom(roomId) }
    }

    @Test
    fun `a cleared screen releases the room`() = runBlocking<Unit> {
        val model = viewModel()

        model.clearForTest()

        verify { interactor.releaseRoom(roomId) }
    }

    @Test
    fun `the new messages marker waits for the ready page to be mapped`() = runBlocking<Unit> {
        coEvery { messageUiMapper.map(match { it.isNotEmpty() }, any(), any(), any(), any(), any(), any()) } returns
            persistentListOf(incomingText("m3"))
        val model = viewModel()
        feed.value = snapshot(readiness = ChatRoomReadiness.READY, messages = listOf(mockk(relaxed = true)))

        val state = model.awaitMessagesState { it.messages.isNotEmpty() && it.firstNewMessageInfo != null }

        assertEquals("m3", state.firstNewMessageInfo?.messageId)
    }

    @Test
    fun `a seen message goes to the source`() = runBlocking<Unit> {
        viewModel().onUnreadMessageVisible(incomingText("m3"))

        coVerify(timeout = 5_000) { interactor.reportMessageSeen(roomId, "m3") }
    }

    private suspend fun ChatFeedViewModel.awaitMessagesState(predicate: (ChatMessagesState) -> Boolean): ChatMessagesState =
        withTimeout(5_000) {
            chatMessagesState.first { (it as? LoadingState.Loaded)?.data?.let(predicate) == true }.let { (it as LoadingState.Loaded).data }
        }

    // `onCleared` is protected, so the test reaches it the way the framework does.
    private fun ChatFeedViewModel.clearForTest() {
        val method = ViewModel::class.java.getDeclaredMethod("onCleared")
        method.isAccessible = true
        method.invoke(this)
    }

    private fun snapshot(
        older: ChatPagingState.Edge = ChatPagingState.Edge.END,
        newer: ChatPagingState.Edge = ChatPagingState.Edge.END,
        readiness: ChatRoomReadiness,
        messages: List<ChatMessage> = emptyList(),
    ) = ChatFeedSnapshot(
        messages = messages,
        reactions = emptyMap(),
        revisions = emptyMap(),
        paging = ChatPagingState(older = older, newer = newer),
        readiness = readiness,
    )

    private fun incomingText(id: String) = ChatMessageUiModel.Text(
        id = id,
        timestamp = 3_000L,
        direction = ChatMessageUiModel.Direction.INCOMING,
        status = ChatMessageUiModel.Status.PENDING,
        text = "message",
        replyPreview = null,
        reactions = persistentListOf(),
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
        isEdited = false,
    )

    private fun viewModel() = chatFeedViewModel(roomId, interactor, messageUiMapper).also { models += it }
}
