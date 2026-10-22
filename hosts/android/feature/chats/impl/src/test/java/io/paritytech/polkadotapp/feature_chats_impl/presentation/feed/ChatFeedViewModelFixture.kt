package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed

import androidx.lifecycle.SavedStateHandle
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatFeedInteractor
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.draft.ChatDraftController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers.ChatMessageUiMapper
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ProductRoomMedia
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.UnconfinedTestDispatcher

/** The room screen of [chatId] with the collaborators that a test gives. */
internal fun chatFeedViewModel(
    chatId: ChatId,
    interactor: ChatFeedInteractor,
    messageUiMapper: ChatMessageUiMapper,
    router: ChatsRouter = mockk(relaxed = true),
): ChatFeedViewModel = ChatFeedViewModel(
    savedStateHandle = SavedStateHandle(mapOf(ChatFeedPayload::class.java.name to ChatFeedPayload.existingChat(chatId))),
    router = router,
    interactor = interactor,
    messageUiMapper = messageUiMapper,
    clipboardService = mockk(relaxed = true),
    menuActionsProvider = ChatMenuActionsProvider(),
    permissionAsker = mockk(relaxed = true),
    attachmentExecutor = mockk(relaxed = true),
    dispatchers = TestCoroutineDispatchers(UnconfinedTestDispatcher()),
    draftControllerFactory = mockk<ChatDraftController.Factory> { every { create(any(), any(), any()) } returns mockk(relaxed = true) },
    callStateTracker = mockk<CallStateTracker> { every { observeActiveCall() } returns flowOf(null) },
    productRoomMediaFactory = ProductRoomMedia.Factory(mockk(relaxed = true), mockk(relaxed = true), mockk(relaxed = true), mockk(relaxed = true)),
)
