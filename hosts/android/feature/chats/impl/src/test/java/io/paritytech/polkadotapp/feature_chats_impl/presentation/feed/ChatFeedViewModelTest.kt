package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatRoomUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.EmptyRoomPlaceholder
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductRoomController
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.Dispatchers
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

class ChatFeedViewModelTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "berlin")
    private val policy = ChatRoomPolicy.Defaults.copy(description = "Forecasts", placeholder = "Ask me")

    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        every { observeRoomPolicy(roomId) } returns flowOf(policy)
        coEvery { triggerMessageAction(any(), any(), any()) } returns Result.success(Unit)
        coEvery { deleteProductRoomMessage(any(), any(), any()) } returns Result.success(ChatSourceActionOutcome.Done)
        coEvery { retryProductRoomMessage(any(), any()) } returns Result.success(ChatSourceActionOutcome.Done)
        coEvery { getLastRevealedTimestamp(any()) } returns null
    }
    private val draftControllerFactory = mockk<ChatDraftController.Factory> {
        every { create(any(), any(), any()) } returns mockk(relaxed = true)
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `the room shows the description and the placeholder of the product`() = runBlocking<Unit> {
        val roomState = withTimeout(5_000) { viewModel().roomState.first { it != ChatRoomUiState.PersonChat } }

        assertEquals(ChatRoomUiState(subtitle = "Forecasts", emptyPlaceholder = EmptyRoomPlaceholder.ProductText("Ask me")), roomState)
    }

    @Test
    fun `a button press reaches the product`() = runBlocking<Unit> {
        viewModel().onMessageAction(MessageAction.ActionPress(actionsMessage(), "yes"))

        coVerify(timeout = 5_000) { interactor.triggerMessageAction(roomId, "message-1", "yes") }
    }

    @Test
    fun `delete asks for confirmation before it deletes`() = runBlocking<Unit> {
        val model = viewModel()

        model.onMessageAction(MessageAction.Delete(actionsMessage()))

        assertEquals(ChatMenuType.DeleteMessageConfirmation("message-1"), model.menuState.value.type)
        coVerify(exactly = 0) { interactor.deleteProductRoomMessage(any(), any(), any()) }

        model.onDeleteMessageConfirm("message-1")

        coVerify(timeout = 5_000) { interactor.deleteProductRoomMessage(roomId, "message-1", policy) }
        assertEquals(false, model.menuState.value.isVisible)
    }

    @Test
    fun `retry goes to the product`() = runBlocking<Unit> {
        viewModel().onMessageAction(MessageAction.Retry(actionsMessage()))

        coVerify(timeout = 5_000) { interactor.retryProductRoomMessage(roomId, "message-1") }
    }

    private fun viewModel() = ChatFeedViewModel(
        savedStateHandle = SavedStateHandle(mapOf(ChatFeedPayload::class.java.name to ChatFeedPayload.existingChat(roomId))),
        router = mockk(relaxed = true),
        interactor = interactor,
        messageUiMapper = mockk(relaxed = true),
        clipboardService = mockk(relaxed = true),
        menuActionsProvider = ChatMenuActionsProvider(),
        permissionAsker = mockk(relaxed = true),
        attachmentExecutor = mockk(relaxed = true),
        dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
        resourceResolver = mockk(relaxed = true),
        productRoomFactory = ChatProductRoomController.Factory(interactor, mockk(relaxed = true), mockk(relaxed = true), mockk(relaxed = true)),
        draftControllerFactory = draftControllerFactory,
        callStateTracker = mockk(relaxed = true),
    )

    private fun actionsMessage() = ChatMessageUiModel.Actions(
        id = "message-1",
        timestamp = 0L,
        direction = ChatMessageUiModel.Direction.INCOMING,
        status = ChatMessageUiModel.Status.SENT,
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
        text = "Rain?",
        buttons = persistentListOf(ChatMessageUiModel.Actions.ActionButton("yes", "Yes")),
        layout = ChatMessageUiModel.Actions.ActionsLayout.COLUMN,
        reactions = persistentListOf(),
    )
}
