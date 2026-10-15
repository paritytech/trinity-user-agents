package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPageOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMessagesState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatPageEdgeUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatRoomReadinessUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductRoomController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.FakeProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.FakeReadAloudController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.callFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.search.ChatRoomSearchController
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
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
import org.junit.Before
import org.junit.Test

/** The paging, readiness and room lifecycle of the room screen of a product-source room (P8.5). */
class ChatFeedViewModelPagingTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")
    private val policy = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)
    private val feed = MutableStateFlow(snapshot(older = ChatPagingState.Edge.LOADING, readiness = ChatRoomReadiness.LOADING))

    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        every { observeRoomPolicy(roomId) } returns flowOf(policy)
        every { subscribeMessages(roomId) } returns feed
        coEvery { getLastRevealedTimestamp(any()) } returns null
        coEvery { loadOlderMessages(any()) } returns ChatPageOutcome.Appended(0)
        coEvery { loadNewerMessages(any()) } returns ChatPageOutcome.Appended(0)
        coEvery { jumpToMessage(any(), any()) } returns false
    }
    private val messageUiMapper = mockk<ChatMessageUiMapper> {
        coEvery { map(any(), any(), any(), any(), any(), any(), any(), any()) } returns persistentListOf()
    }
    private val draftControllerFactory = mockk<ChatDraftController.Factory> {
        every { create(any(), any(), any()) } returns mockk(relaxed = true)
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
        feed.value = snapshot(older = ChatPagingState.Edge.MORE, newer = ChatPagingState.Edge.FAILED, readiness = ChatRoomReadiness.UNAVAILABLE)

        val state = model.awaitMessagesState { it.readiness == ChatRoomReadinessUiState.UNAVAILABLE }

        assertEquals(ChatPageEdgeUiState.MORE, state.olderPage)
        assertEquals(ChatPageEdgeUiState.FAILED, state.newerPage)
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
    fun `a failed newer page can be asked again`() = runBlocking<Unit> {
        val model = viewModel()
        feed.value = snapshot(newer = ChatPagingState.Edge.FAILED, readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.newerPage == ChatPageEdgeUiState.FAILED }

        model.onLoadNewerMessages()

        coVerify(timeout = 5_000) { interactor.loadNewerMessages(roomId) }
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
    fun `S17 a tap on a quote of an unloaded message asks the source to jump`() = runBlocking<Unit> {
        val model = viewModel()
        feed.value = snapshot(readiness = ChatRoomReadiness.READY)
        model.awaitMessagesState { it.readiness == ChatRoomReadinessUiState.READY }

        model.onMessageAction(MessageAction.ReplyPreviewTap("m10"))

        coVerify(timeout = 5_000) { interactor.jumpToMessage(roomId, "m10") }
    }

    @Test
    fun `the screen opens the room on start, closes it on stop and retries it`() = runBlocking<Unit> {
        val model = viewModel()

        model.onScreenStarted()
        model.onScreenStopped()
        model.onRetryRoom()

        verify { interactor.openRoom(roomId) }
        verify { interactor.closeRoom(roomId) }
        verify { interactor.retryRoom(roomId) }
    }

    @Test
    fun `a cleared screen releases the room`() = runBlocking<Unit> {
        val model = viewModel()

        model.clearForTest()

        verify { interactor.releaseRoom(roomId) }
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
    ) = ChatFeedSnapshot(
        messages = emptyList(),
        reactions = emptyMap(),
        revisions = emptyMap(),
        productStatuses = emptyMap(),
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
        isMarkdown = false,
    )

    private fun viewModel() = createViewModel().also { models += it }

    private fun createViewModel() = ChatFeedViewModel(
        savedStateHandle = SavedStateHandle(mapOf(ChatFeedPayload::class.java.name to ChatFeedPayload.existingChat(roomId))),
        router = mockk(relaxed = true),
        interactor = interactor,
        messageUiMapper = messageUiMapper,
        clipboardService = mockk(relaxed = true),
        menuActionsProvider = ChatMenuActionsProvider(),
        permissionAsker = mockk(relaxed = true),
        attachmentExecutor = mockk(relaxed = true),
        dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
        resourceResolver = mockk(relaxed = true),
        productRoomFactory = ChatProductRoomController.Factory(
            interactor,
            mockk(relaxed = true),
            mockk(relaxed = true),
            mockk(relaxed = true),
            mockk(relaxed = true),
            FakeProductRoomAdditionsFeed(),
            FakeReadAloudController(),
            mockk(relaxed = true),
            callFactory(),
        ),
        draftControllerFactory = draftControllerFactory,
        callStateTracker = mockk<CallStateTracker> { every { observeActiveCall() } returns flowOf(null) },
        roomSearchFactory = ChatRoomSearchController.Factory(mockk(relaxed = true)),
    )
}
