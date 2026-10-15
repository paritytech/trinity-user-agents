package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatFooterRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatToolbarActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatMessageMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatReactionDetailModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatSourceActionOutcome
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay.MessageOriginDisplayResolver
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatInputUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantAppearanceFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductLinkOpener
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatProductRoomController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ParticipantSheetEntry
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductRoomAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductToolbarItemUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.callFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.search.ChatRoomSearchController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
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
    private val feed = MutableStateFlow(snapshot)
    private val activity = MutableStateFlow<ChatRoomActivityModel?>(null)
    private val roomExists = MutableStateFlow(true)
    private val router = mockk<ChatsRouter>(relaxed = true)
    private val origins = object : MessageOriginDisplayResolver {
        override fun displayOf(messageOrigin: ChatMessageOrigin) = MessageOriginDisplay("Weather", ChatAvatar.Url("https://weather"))
    }
    private val querying = mockk<ProductChatQuerying> {
        coEvery { participantsCount(any()) } returns Result.success(3)
        coEvery { participantsByIds(any(), any()) } answers { Result.success(secondArg<List<String>>().map(::participant)) }
        coEvery { participantsPage(any(), null, 100) } returns Result.success(
            ProductParticipantsPage(listOf(participant("alice"), participant("bob")), next = null),
        )
    }
    private val menu = ChatMessageMenuModel.Custom(
        listOf(
            productAction("pin", ChatActionHandlerModel.Callback),
            productAction("docs", ChatActionHandlerModel.Deeplink("polkadot://weather.dot/docs")),
        ),
    )

    private val interactor = mockk<ChatFeedInteractor>(relaxed = true) {
        every { observeRoomPolicy(roomId) } returns policy
        every { subscribeMessages(roomId) } returns feed
        coEvery { getMessageOriginDisplayResolver(any()) } returns origins
        coEvery { roomQuerying(roomId) } returns querying
        every { subscribeUserInputState(any()) } returns flowOf(ChatUserInputState.Nothing)
        every { observeRoomActivity(any()) } returns activity
        every { observeRoomExists(roomId) } returns roomExists
        coEvery { sendTextMessage(any(), any(), any()) } returns ChatSourceActionOutcome.Done
        coEvery { getLastRevealedTimestamp(any()) } returns null
        coEvery { productMessageMenu(any(), any(), any()) } returns Result.success(menu)
        coEvery { sendUserEvent(any(), any()) } returns Result.success(Unit)
    }
    private val productLinkOpener = mockk<ChatProductLinkOpener>(relaxed = true)
    private val clipboardService = mockk<ClipboardService>(relaxed = true)
    private val resourceResolver = mockk<ChatResourceResolver> {
        coEvery { resolve(any()) } returns Result.failure(IllegalStateException("no image in a unit test"))
    }
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

    @Test
    fun `a link of a message opens only after the user confirms it`() = runBlocking<Unit> {
        val model = viewModel()

        model.onProductRoomAction(ProductRoomAction.ConfirmLink("https://weather.example/radar"))

        assertEquals(ChatMenuType.ProductLinkConfirmation("https://weather.example/radar"), model.menuState.value.type)
        with(model) { coVerify(exactly = 0) { productLinkOpener.open(any()) } }

        model.onProductRoomAction(ProductRoomAction.OpenLink("https://weather.example/radar"))

        with(model) { coVerify(timeout = 5_000) { productLinkOpener.open("https://weather.example/radar") } }
    }

    @Test
    fun `a link of a message with another scheme than http does not open`() = runBlocking<Unit> {
        val model = viewModel()

        model.onProductRoomAction(ProductRoomAction.ConfirmLink("intent://scan#Intent;end"))

        assertEquals(null, model.menuState.value.type)
    }

    @Test
    fun `the toolbar shows the buttons of the product`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(toolbar = listOf(toolbarAction("refresh", ChatActionHandlerModel.Callback)))

        val toolbar = withTimeout(5_000) { viewModel().productRoomState.first { it.toolbar.inline.isNotEmpty() }.toolbar }

        assertEquals(listOf(ProductToolbarItemUiModel("refresh", "refresh", icon = null)), toolbar.inline)
    }

    @Test
    fun `a toolbar button sends ToolbarActionTriggered, or asks to open its link`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(
            toolbar = listOf(
                toolbarAction("refresh", ChatActionHandlerModel.Callback),
                toolbarAction("help", ChatActionHandlerModel.Deeplink("polkadot://weather.dot/help")),
            ),
        )
        val model = viewModel()
        withTimeout(5_000) { model.productRoomState.first { it.toolbar.inline.size == 2 } }

        model.onProductRoomAction(ProductRoomAction.Toolbar("refresh"))
        model.onProductRoomAction(ProductRoomAction.Toolbar("help"))

        coVerify(timeout = 5_000) { interactor.sendUserEvent(roomId, ChatRoomUserEvent.ToolbarActionTriggered("refresh")) }
        coVerify(exactly = 1) { interactor.sendUserEvent(any(), any()) }
        assertEquals(ChatMenuType.ProductLinkConfirmation("polkadot://weather.dot/help"), model.menuState.value.type)
    }

    @Test
    fun `a room that declares its participants counts them, names the authors and lists them`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(productQueries = setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS))
        feed.value = snapshot.copy(messages = listOf(storedMessage.copy(originParticipantId = "alice")))
        val model = viewModel()

        val participants = withTimeout(5_000) {
            model.productRoomState.first { it.participants.count == 3 && it.participants.authors.isNotEmpty() }.participants
        }

        assertEquals(true, participants.declared)
        assertEquals("Alice", participants.authors.getValue("message-1").name)

        model.onProductRoomAction(ProductRoomAction.ShowParticipants)

        assertEquals(ChatMenuType.Participants, model.menuState.value.type)
        val list = withTimeout(5_000) { model.productRoomState.first { it.participants.list.participants.size == 2 }.participants.list }
        assertEquals(listOf("Alice", "Bob"), list.participants.map { it.name })
        assertEquals(false, list.hasMore)
    }

    @Test
    fun `a room that does not declare its participants shows no count and no authors`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults
        feed.value = snapshot.copy(messages = listOf(storedMessage.copy(originParticipantId = "alice")))

        val participants = viewModel().productRoomState.value.participants

        assertEquals(false, participants.declared)
        assertEquals(null, participants.count)
        coVerify(exactly = 0) { querying.participantsCount(any()) }
    }

    @Test
    fun `the sheet of a participant shows the entries of the product, and a callback entry sends ParticipantActionTriggered`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(
            productQueries = setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS, ChatRoomPolicy.ProductQuery.PARTICIPANT_ACTIONS),
        )
        coEvery { querying.participantMenu(any(), any()) } returns Result.success(
            ChatParticipantMenuModel.Custom(listOf(productAction("mute", ChatActionHandlerModel.Callback))),
        )
        val model = viewModel()

        model.onProductRoomAction(ProductRoomAction.SelectParticipant("alice"))

        val sheet = withTimeout(5_000) { model.productRoomState.first { it.participantSheet?.loading == false }.participantSheet!! }
        assertEquals("Alice", sheet.participant.name)
        assertEquals(listOf(ParticipantSheetEntry.Product("mute", "mute", icon = null, isDestructive = false)), sheet.entries)
        assertEquals(ChatMenuType.ParticipantSheet, model.menuState.value.type)
        coVerify { querying.participantMenu(any(), participant("alice")) }

        model.onProductRoomAction(ProductRoomAction.ParticipantSheet(sheet.entries.single()))

        coVerify(timeout = 5_000) { interactor.sendUserEvent(roomId, ChatRoomUserEvent.ParticipantActionTriggered("alice", "mute")) }
    }

    @Test
    fun `the sheet of a lite person copies its address`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(productQueries = setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS))
        val account = ByteArray(32) { 1 }.toDataByteArray()
        coEvery { querying.participantsByIds(any(), listOf("lite")) } returns
            Result.success(listOf(ChatParticipantEntryModel("lite", ChatParticipantModel.LitePerson(account))))
        val model = viewModel()

        model.onProductRoomAction(ProductRoomAction.SelectParticipant("lite"))

        val sheet = withTimeout(5_000) { model.productRoomState.first { it.participantSheet != null }.participantSheet!! }
        val copy = sheet.entries.single() as ParticipantSheetEntry.CopyAddress
        model.onProductRoomAction(ProductRoomAction.ParticipantSheet(copy))

        verify { clipboardService.setPrimaryClip(copy.address) }
        coVerify(exactly = 0) { querying.participantMenu(any(), any()) }
    }

    @Test
    fun `who reacted comes from the product, and NotAvailable hides it for the rest of the screen`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(
            offersReactionDetails = true,
            productQueries = setOf(ChatRoomPolicy.ProductQuery.PARTICIPANTS),
        )
        coEvery { querying.reactions(any(), "message-1") } returnsMany listOf(
            Result.success(listOf(ChatReactionDetailModel("alice", "👍"))),
            Result.failure(ProductChatQueryException.NotAvailable()),
        )
        val model = viewModel()

        model.onMessageAction(MessageAction.ShowReactionDetails(uiMessage()))

        val details = withTimeout(5_000) { model.popupState.first { it is MessagePopUpUiState.ReactionsDetails } }
            as MessagePopUpUiState.ReactionsDetails
        assertEquals("Alice", details.reactionsByEmoji.single().reactions.single().authorName)

        model.onMessageAction(MessageAction.DismissActionMenu)
        model.onMessageAction(MessageAction.ShowReactionDetails(uiMessage()))
        model.onMessageAction(MessageAction.ShowReactionDetails(uiMessage()))

        coVerify(timeout = 5_000, exactly = 2) { querying.reactions(any(), "message-1") }
        assertEquals(null, model.popupState.value)
    }

    @Test
    fun `S47 a configuration that changes while the room is open changes the toolbar, the input and the menu`() = runBlocking<Unit> {
        every { interactor.subscribeUserInputState(any()) } returns flowOf(
            ChatUserInputState.SendMessage(ChatUserInputState.SendMessage.Payment.NotAvailable, attachmentsSupported = false),
        )
        val model = viewModel()
        withTimeout(5_000) { model.chatInputUiState.first { it is ChatInputUiState.SendMessage } }

        val input = ChatRoomPolicy.DefaultInput.copy(placeholder = "Ask again")
        policy.value = policy.value.copy(
            configGeneration = 2,
            toolbar = listOf(toolbarAction("refresh", ChatActionHandlerModel.Callback)),
            footer = ChatRoomPolicy.Footer.TextInput(input),
            productQueries = emptySet(),
        )

        withTimeout(5_000) { model.productRoomState.first { it.toolbar.inline.isNotEmpty() } }
        val state = withTimeout(5_000) {
            model.chatInputUiState.first { (it as? ChatInputUiState.SendMessage)?.placeholder == "Ask again" }
        }
        assertEquals("Ask again", (state as ChatInputUiState.SendMessage).placeholder)

        model.onMessageAction(MessageAction.LongPress(uiMessage()))
        coVerify(exactly = 0) { interactor.productMessageMenu(any(), any(), any()) }
    }

    @Test
    fun `a new configuration asks the product again who reacted after NotAvailable`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(offersReactionDetails = true)
        coEvery { querying.reactions(any(), "message-1") } returns Result.failure(ProductChatQueryException.NotAvailable())
        val model = viewModel()

        model.onMessageAction(MessageAction.ShowReactionDetails(uiMessage()))
        coVerify(timeout = 5_000, exactly = 1) { querying.reactions(any(), "message-1") }
        model.onMessageAction(MessageAction.ShowReactionDetails(uiMessage()))
        coVerify(exactly = 1) { querying.reactions(any(), "message-1") }

        policy.value = policy.value.copy(configGeneration = 3)
        withTimeout(5_000) { model.productRoomState.first() }
        delay(100)
        model.onMessageAction(MessageAction.ShowReactionDetails(uiMessage()))

        coVerify(timeout = 5_000, exactly = 2) { querying.reactions(any(), "message-1") }
    }

    @Test
    fun `the input of a product room shows the placeholder and the attach button of its footer`() = runBlocking<Unit> {
        val input = ChatRoomPolicy.DefaultInput.copy(placeholder = "Ask the weather", allowsPhoto = false, allowsVideo = false, allowsFile = true)
        policy.value = ChatRoomPolicy.Defaults.copy(footer = ChatRoomPolicy.Footer.TextInput(input))
        every { interactor.subscribeUserInputState(any()) } returns flowOf(
            ChatUserInputState.SendMessage(ChatUserInputState.SendMessage.Payment.NotAvailable, attachmentsSupported = false),
        )

        val state = withTimeout(5_000) { viewModel().chatInputUiState.first { it is ChatInputUiState.SendMessage } } as ChatInputUiState.SendMessage

        assertEquals("Ask the weather", state.placeholder)
        assertEquals(true, state.showAttachButton)
        assertEquals(false, state.showPayButton)
    }

    @Test
    fun `a custom footer draws in the footer slot and not in the list`() = runBlocking<Unit> {
        val renderer = mockk<CustomChatFooterRenderer>()
        coEvery { interactor.getFooterRendererForChat(roomId) } returns renderer
        policy.value = ChatRoomPolicy.Defaults.copy(footer = ChatRoomPolicy.Footer.Custom)
        val model = viewModel()

        val footer = withTimeout(5_000) { model.productRoomState.first { it.customFooter != null }.customFooter }

        assertEquals(renderer, footer)
        assertEquals(null, model.footerRenderer.value)
    }

    @Test
    fun `a quick reply that sends its text sends it, and another one sends SuggestionSelected`() = runBlocking<Unit> {
        policy.value = ChatRoomPolicy.Defaults.copy(
            suggestions = listOf(ChatSuggestionModel("yes", "Yes please", sendsText = true), ChatSuggestionModel("more", "More", sendsText = false)),
        )
        val model = viewModel()

        model.onProductRoomAction(ProductRoomAction.Suggestion("yes"))
        model.onProductRoomAction(ProductRoomAction.Suggestion("more"))

        coVerify(timeout = 5_000) { interactor.sendTextMessage(roomId, "Yes please", null) }
        coVerify(timeout = 5_000) { interactor.sendUserEvent(roomId, ChatRoomUserEvent.SuggestionSelected("more")) }
        coVerify(exactly = 1) { interactor.sendTextMessage(any(), any(), any()) }
    }

    @Test
    fun `the room shows the suggestions, the commands and the activity line`() = runBlocking<Unit> {
        val command = ChatCommandModel("forecast", "The forecast of a city", argumentHint = "<city>")
        policy.value = ChatRoomPolicy.Defaults.copy(suggestions = listOf(ChatSuggestionModel("yes", "Yes", sendsText = true)), commands = listOf(command))
        val model = viewModel()

        activity.value = ChatRoomActivityModel("Weather is typing", participantId = null)

        val extras = withTimeout(5_000) { model.productRoomState.first { it.extras.activity != null }.extras }
        assertEquals("Weather is typing", extras.activity)
        assertEquals(listOf(command), extras.commands)
        assertEquals(listOf("yes"), extras.suggestions.map { it.suggestionId })

        activity.value = null
        withTimeout(5_000) { model.productRoomState.first { it.extras.activity == null } }
    }

    @Test
    fun `a room that goes away while it is open closes its screen`() = runBlocking<Unit> {
        viewModel()
        verify(exactly = 0) { router.back() }

        roomExists.value = false

        verify(timeout = 5_000) { router.back() }
    }

    @Test
    fun `closing the room screen closes the room`() = runBlocking<Unit> {
        viewModel().onScreenStopped()

        verify { interactor.closeRoom(roomId) }
    }

    private fun participant(id: String) =
        ChatParticipantEntryModel(id, ChatParticipantModel.Custom(nickname = id.replaceFirstChar { it.uppercase() }, avatar = null))

    private fun toolbarAction(id: String, handler: ChatActionHandlerModel) =
        ChatToolbarActionModel(actionId = id, label = id, icon = ChatResourceModel.Inline("image/png", byteArrayOf(1).toDataByteArray()), handler = handler)

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
        router = router,
        interactor = interactor,
        messageUiMapper = mockk(relaxed = true),
        clipboardService = mockk(relaxed = true),
        menuActionsProvider = ChatMenuActionsProvider(),
        permissionAsker = mockk(relaxed = true),
        attachmentExecutor = mockk(relaxed = true),
        dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
        resourceResolver = mockk(relaxed = true),
        productRoomFactory = ChatProductRoomController.Factory(
            interactor = interactor,
            resourceResolver = resourceResolver,
            productLinkOpener = productLinkOpener,
            appearances = ChatParticipantAppearanceFactory(mockk(relaxed = true), resourceResolver),
            clipboardService = clipboardService,
            callFactory = callFactory(),
        ),
        draftControllerFactory = draftControllerFactory,
        callStateTracker = mockk<CallStateTracker> { every { observeActiveCall() } returns flowOf(null) },
        roomSearchFactory = ChatRoomSearchController.Factory(mockk(relaxed = true)),
    )
}
