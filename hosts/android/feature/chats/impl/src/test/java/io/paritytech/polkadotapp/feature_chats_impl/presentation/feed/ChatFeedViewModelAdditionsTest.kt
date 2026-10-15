package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageMetaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatReasoningModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.computeChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.AllowedMessageMenuAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.toOpenChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantAppearanceFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductRoomController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.FakeProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.FakeReadAloudController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.callFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.search.ChatRoomSearchController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentSetOf
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
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Test

/** The bot personas, the message meta and "Read aloud" on the room screen of a product room (07-LATER §5, §9). */
class ChatFeedViewModelAdditionsTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val policy = MutableStateFlow(ChatRoomPolicy.Defaults)
    private val storedMessage = ChatMessage(
        id = "message-1",
        chatId = roomId,
        timestamp = 1_000L,
        origin = ChatMessageOrigin.Extension("ProductBot_sample.dot"),
        content = ChatMessage.Content.Text("Hello from the persona"),
        status = ChatMessage.Status.IS_READ,
        originParticipantId = "bot:helper",
    )
    private val feed = MutableStateFlow(
        ChatFeedSnapshot(listOf(storedMessage), emptyMap(), emptyMap(), emptyMap(), ChatPagingState.Complete, ChatRoomReadiness.READY),
    )
    private val activity = MutableStateFlow<ChatRoomActivityModel?>(null)
    private val additions = FakeProductRoomAdditionsFeed()
    private val readAloud = FakeReadAloudController()
    private val router = mockk<ChatsRouter>(relaxed = true)
    private val querying = mockk<ProductChatQuerying>(relaxed = true)
    private val origins = object : MessageOriginDisplayResolver {
        override fun displayOf(messageOrigin: ChatMessageOrigin) = MessageOriginDisplay("Sample", ChatAvatar.Url("https://sample"))
    }
    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        every { observeRoomPolicy(roomId) } returns policy
        every { subscribeMessages(roomId) } returns feed
        coEvery { getMessageOriginDisplayResolver(any()) } returns origins
        coEvery { roomQuerying(roomId) } returns querying
        every { subscribeUserInputState(any()) } returns flowOf(ChatUserInputState.Nothing)
        every { observeRoomActivity(any()) } returns activity
        every { observeRoomExists(roomId) } returns flowOf(true)
        coEvery { getLastRevealedTimestamp(any()) } returns null
    }
    private val resourceResolver = mockk<ChatResourceResolver> {
        every { resolveIcon(any()) } returns Result.failure(IllegalStateException("no image in a unit test"))
    }
    private val messageUiMapper = mockk<ChatMessageUiMapper>(relaxed = true) {
        every { constructPopUpState(any(), any(), any(), any(), any(), any(), any()) } answers {
            MessagePopUpUiState.ActionMenu(
                message = uiMessage(),
                userReactedEmojis = persistentSetOf(),
                canLeaveReactions = true,
                allowedMenuActions = persistentListOf(AllowedMessageMenuAction.Reply, AllowedMessageMenuAction.Copy("Hello from the persona")),
                allowedReactions = null,
                productActionsLoading = false,
            )
        }
    }
    private val draftControllerFactory = mockk<ChatDraftController.Factory> {
        every { create(any(), any(), any()) } returns mockk(relaxed = true)
    }

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        additions.personas.value = listOf(ChatBotPersonaModel("helper", "Helper", "data:image/png;base64,AA"))
    }

    @After
    fun tearDown() {
        // A view model that a test leaves open would run on the main dispatcher of the next test.
        models.forEach { it.viewModelScope.cancel() }
        Dispatchers.resetMain()
    }

    private val models = mutableListOf<ChatFeedViewModel>()

    @Test
    fun `a persona message draws the name of the persona with no participants query`() = runBlocking<Unit> {
        val model = viewModel()

        val author = withTimeout(5_000) { model.productRoomState.first { it.participants.authors.isNotEmpty() } }.participants.authors["message-1"]

        assertEquals("Helper", author?.name)
        coVerify(exactly = 0) { querying.participantsByIds(any(), any()) }
    }

    @Test
    fun `the activity line of a persona names the persona`() = runBlocking<Unit> {
        val model = viewModel()

        activity.value = ChatRoomActivityModel("typing", participantId = "bot:helper")

        val extras = withTimeout(5_000) { model.productRoomState.first { it.extras.activity != null }.extras }
        assertEquals("Helper: typing", extras.activity)
        assertEquals("Helper", extras.activitySpeaker?.name)
    }

    @Test
    fun `the menu of a reasoning row offers Copy only`() = runBlocking<Unit> {
        additions.meta.value = mapOf("message-1" to ChatMessageMetaModel(null, null, ChatReasoningModel(durationMs = 4_000)))
        val model = viewModel()
        withTimeout(5_000) { model.productRoomState.first { it.decorations.isNotEmpty() } }

        model.onMessageAction(MessageAction.LongPress(uiMessage()))

        val menu = withTimeout(5_000) { model.popupState.first { it != null } } as MessagePopUpUiState.ActionMenu
        assertEquals(listOf(AllowedMessageMenuAction.Copy("Hello from the persona")), menu.allowedMenuActions)
        assertFalse(menu.canLeaveReactions)
    }

    @Test
    fun `Read aloud is in the menu only when the room declares it, and leaving the room stops it`() = runBlocking<Unit> {
        val model = viewModel()
        model.onMessageAction(MessageAction.LongPress(uiMessage()))
        val without = withTimeout(5_000) { model.popupState.first { it != null } } as MessagePopUpUiState.ActionMenu
        assertFalse(without.allowedMenuActions.any { it is AllowedMessageMenuAction.ReadAloud })

        policy.value = ChatRoomPolicy.Defaults.copy(features = ChatRoomFeaturesModel.HOST_DEFAULT.copy(readAloud = true))
        val with = withTimeout(5_000) {
            model.popupState.first { state -> (state as? MessagePopUpUiState.ActionMenu)?.allowedMenuActions?.any { it is AllowedMessageMenuAction.ReadAloud } == true }
        } as MessagePopUpUiState.ActionMenu
        assertEquals(AllowedMessageMenuAction.ReadAloud("Hello from the persona"), with.allowedMenuActions.last())

        model.onMessageAction(MessageAction.ReadAloud("Hello from the persona"))
        assertEquals(listOf("Hello from the persona"), readAloud.spoken)
        withTimeout(5_000) { model.productRoomState.first { it.readingAloud } }

        model.onProductRoomAction(ProductRoomAction.StopReadAloud)
        withTimeout(5_000) { model.productRoomState.first { !it.readingAloud } }
    }

    @Test
    fun `a thread chip opens the thread room on top of the room`() = runBlocking<Unit> {
        val thread = ChatId.forExtensionRoom("ProductBot_sample.dot", "thread-host-message-1")

        viewModel().onProductRoomAction(ProductRoomAction.OpenThread(thread))

        verify { router.openThreadFeed(match { it.toOpenChatRequest().computeChatId() == thread }) }
    }

    private fun uiMessage() = ChatMessageUiModel.Text(
        id = "message-1",
        timestamp = 1_000L,
        direction = ChatMessageUiModel.Direction.INCOMING,
        status = ChatMessageUiModel.Status.READ,
        origin = ChatMessageOrigin.Extension("ProductBot_sample.dot"),
        text = "Hello from the persona",
        replyPreview = null,
        reactions = persistentListOf(),
        isEdited = false,
        isMarkdown = false,
    )

    private fun viewModel() = createViewModel().also { models += it }

    private fun createViewModel() = ChatFeedViewModel(
        savedStateHandle = SavedStateHandle(mapOf(ChatFeedPayload::class.java.name to ChatFeedPayload.existingChat(roomId))),
        router = router,
        interactor = interactor,
        messageUiMapper = messageUiMapper,
        clipboardService = mockk(relaxed = true),
        menuActionsProvider = ChatMenuActionsProvider(),
        permissionAsker = mockk(relaxed = true),
        attachmentExecutor = mockk(relaxed = true),
        dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
        resourceResolver = mockk(relaxed = true),
        productRoomFactory = ChatProductRoomController.Factory(
            interactor = interactor,
            resourceResolver = resourceResolver,
            productLinkOpener = mockk(relaxed = true),
            appearances = ChatParticipantAppearanceFactory(mockk(relaxed = true), resourceResolver),
            clipboardService = mockk(relaxed = true),
            additions = additions,
            readAloud = readAloud,
            router = router,
            callFactory = callFactory(),
        ),
        draftControllerFactory = draftControllerFactory,
        callStateTracker = mockk<CallStateTracker> { every { observeActiveCall() } returns flowOf(null) },
        roomSearchFactory = ChatRoomSearchController.Factory(mockk(relaxed = true)),
    )
}
