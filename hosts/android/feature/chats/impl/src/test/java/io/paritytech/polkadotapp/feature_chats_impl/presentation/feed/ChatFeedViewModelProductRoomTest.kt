package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductLinkOpener
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductRoomController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomAction
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

/** The menus, toolbar, participants, footer and extras of the room screen of a product room (P8.6). */
class ChatFeedViewModelProductRoomTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")
    private val policy = MutableStateFlow(
        ChatRoomPolicy.Defaults.copy(productQueries = setOf(ChatRoomPolicy.ProductQuery.MESSAGE_ACTIONS)),
    )
    private val storedMessage = ChatMessage(
        id = "message-1",
        chatId = roomId,
        timestamp = 1_000L,
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
        content = ChatMessage.Content.Text("Rain today"),
        status = ChatMessage.Status.IS_READ,
    )
    private val snapshot = ChatFeedSnapshot(
        messages = listOf(storedMessage),
        reactions = emptyMap(),
        revisions = emptyMap(),
        productStatuses = emptyMap(),
        paging = ChatPagingState.Complete,
        readiness = ChatRoomReadiness.READY,
    )
    private val menu = ChatMessageMenuModel.Custom(
        listOf(
            productAction("pin", ChatActionHandlerModel.Callback),
            productAction("docs", ChatActionHandlerModel.Deeplink("polkadot://weather.dot/docs")),
        ),
    )

    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        every { observeRoomPolicy(roomId) } returns policy
        every { subscribeMessages(roomId) } returns flowOf(snapshot)
        coEvery { getLastRevealedTimestamp(any()) } returns null
        coEvery { productMessageMenu(any(), any(), any()) } returns Result.success(menu)
        coEvery { sendUserEvent(any(), any()) } returns Result.success(Unit)
    }
    private val productLinkOpener = mockk<ChatProductLinkOpener>(relaxed = true)
    private val draftControllerFactory = mockk<ChatDraftController.Factory> {
        every { create(any(), any(), any()) } returns mockk(relaxed = true)
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `a long press asks the product for the menu of the message`() = runBlocking<Unit> {
        viewModel().onMessageAction(MessageAction.LongPress(uiMessage()))

        coVerify(timeout = 5_000) { interactor.productMessageMenu(roomId, storedMessage, snapshot) }
    }

    @Test
    fun `a room that does not declare message actions does not ask the product`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults

        viewModel().onMessageAction(MessageAction.LongPress(uiMessage()))

        coVerify(exactly = 0) { interactor.productMessageMenu(any(), any(), any()) }
    }

    @Test
    fun `a callback entry sends MessageActionTriggered`() = runBlocking<Unit> {
        val model = viewModel()
        model.onMessageAction(MessageAction.LongPress(uiMessage()))

        model.onMessageAction(MessageAction.ProductMenuAction(uiMessage(), "pin"))

        coVerify(timeout = 5_000) { interactor.sendUserEvent(roomId, ChatRoomUserEvent.MessageActionTriggered("message-1", "pin")) }
    }

    @Test
    fun `a link entry opens after the user confirms it`() = runBlocking<Unit> {
        val model = viewModel()
        model.onMessageAction(MessageAction.LongPress(uiMessage()))

        model.onMessageAction(MessageAction.ProductMenuAction(uiMessage(), "docs"))

        assertEquals(ChatMenuType.ProductLinkConfirmation("polkadot://weather.dot/docs"), model.menuState.value.type)
        coVerify(exactly = 0) { interactor.sendUserEvent(any(), any()) }

        model.onProductRoomAction(ProductRoomAction.OpenLink("polkadot://weather.dot/docs"))

        with(model) { coVerify(timeout = 5_000) { productLinkOpener.open("polkadot://weather.dot/docs") } }
    }

    private fun productAction(id: String, handler: ChatActionHandlerModel) =
        ChatProductActionModel(actionId = id, label = id, icon = null, destructive = false, handler = handler)

    private fun uiMessage() = ChatMessageUiModel.Text(
        id = "message-1",
        timestamp = 1_000L,
        direction = ChatMessageUiModel.Direction.INCOMING,
        status = ChatMessageUiModel.Status.READ,
        origin = ChatMessageOrigin.Extension("ProductBot_weather"),
        text = "Rain today",
        replyPreview = null,
        reactions = persistentListOf(),
        isEdited = false,
        isMarkdown = false,
    )

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
        productRoomFactory = ChatProductRoomController.Factory(interactor, mockk(relaxed = true), productLinkOpener),
        draftControllerFactory = draftControllerFactory,
        callStateTracker = mockk<CallStateTracker> { every { observeActiveCall() } returns flowOf(null) },
    )
}
