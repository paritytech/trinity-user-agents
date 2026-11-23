package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatUserInputState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils.AttachmentExecutor
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
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
import org.mockito.Mockito.RETURNS_DEFAULTS
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import org.mockito.Mockito.withSettings
import kotlin.time.Duration.Companion.seconds

private const val USERNAME = "Maxwell.42"
private val STATE_TIMEOUT = 1.seconds

@OptIn(ExperimentalCoroutinesApi::class)
class ChatFeedViewModelTest {
    private val contactAccountId: AccountId = ByteArray(32) { 7 }.toDataByteArray()
    private val chatDisplay = MutableSharedFlow<Result<ChatDisplay>>(replay = 1)
    private val userInputState = MutableStateFlow<ChatUserInputState>(ChatUserInputState.AcceptChatRequest)

    private val router: ChatsRouter = mock(ChatsRouter::class.java)
    private val interactor: ChatFeedInteractor = mockWithIdleFlows()
    private val draftControllerFactory: ChatDraftController.Factory = mock(ChatDraftController.Factory::class.java)

    private lateinit var viewModel: ChatFeedViewModel

    @Before
    fun setUp() = runBlocking<Unit> {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        whenever(interactor.observeChatDisplay(any())).thenReturn(chatDisplay)
        whenever(interactor.subscribeUserInputState(any())).thenReturn(userInputState)
        whenever(interactor.declineIncomingRequest(any())).thenReturn(Result.success(Unit))
        whenever(draftControllerFactory.create(any(), any(), any())).thenReturn(mock(ChatDraftController::class.java))
        viewModel = createViewModel()
        withLoadedChatDisplay()
    }

    @After
    fun tearDown() {
        viewModel.viewModelScope.cancel()
        Dispatchers.resetMain()
    }

    @Test
    fun `decline asks first and declines once on confirm`() = runBlocking<Unit> {
        viewModel.onDeclineChatRequest()
        assertDeclineConfirmationShown()
        verifyNotDeclined()

        viewModel.onDeclineChatRequestConfirm()
        viewModel.onDeclineChatRequestConfirm()

        verify(interactor).declineIncomingRequest(eq(contactAccountId))

        userInputState.value = ChatUserInputState.ChatRemoved
        verify(router).back()
    }

    private suspend fun withLoadedChatDisplay() {
        chatDisplay.emit(Result.success(ChatDisplay(USERNAME, ChatAvatar.Account(USERNAME, themeSeed = contactAccountId.value))))
        withTimeout(STATE_TIMEOUT) { viewModel.chatDisplay.first { it is LoadingState.Loaded } }
    }

    private suspend fun assertDeclineConfirmationShown() {
        val state = withTimeout(STATE_TIMEOUT) { viewModel.menuState.first { it.isVisible } }
        assertEquals(ChatMenuType.DeclineRequestConfirmation(USERNAME), state.type)
    }

    private suspend fun verifyNotDeclined() {
        verify(interactor, never()).declineIncomingRequest(any())
    }

    private fun createViewModel() = ChatFeedViewModel(
        savedStateHandle = SavedStateHandle(
            mapOf(ChatFeedPayload::class.java.name to ChatFeedPayload.existingContactChat(contactAccountId))
        ),
        router = router,
        interactor = interactor,
        messageUiMapper = mock(ChatMessageUiMapper::class.java),
        clipboardService = mock(ClipboardService::class.java),
        menuActionsProvider = mock(ChatMenuActionsProvider::class.java),
        permissionAsker = mock(PermissionAsker::class.java),
        attachmentExecutor = mock(AttachmentExecutor::class.java),
        dispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
        draftControllerFactory = draftControllerFactory,
        callStateTracker = mockWithIdleFlows()
    )

    private inline fun <reified T> mockWithIdleFlows(): T = mock(
        T::class.java,
        withSettings().defaultAnswer { invocation ->
            if (Flow::class.java.isAssignableFrom(invocation.method.returnType)) emptyFlow<Any>() else RETURNS_DEFAULTS.answer(invocation)
        }
    )
}
