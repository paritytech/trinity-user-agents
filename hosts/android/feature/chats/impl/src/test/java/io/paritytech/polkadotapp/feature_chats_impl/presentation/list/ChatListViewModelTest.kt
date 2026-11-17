package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import androidx.lifecycle.viewModelScope
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderersById
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ActivityPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraft
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraftRelation
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
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

class ChatListViewModelTest {
    private val room = ChatId.forExtensionRoom("ProductBot_chess", "lobby")
    private val models = mutableListOf<ChatListViewModel>()

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() {
        models.forEach { it.viewModelScope.cancel() }
        Dispatchers.resetMain()
    }

    // The activity is live, so the row shows it in place of the last message and of a draft of the user.
    @Test
    fun `an activity replaces the preview line, also over a draft`() = runBlocking<Unit> {
        val chat = chat(activity = ChatRoomActivityModel("Ben is typing", ongoing = true), badge = ChatSummaryBadge.None)

        val row = rowOf(chat, drafts = mapOf(room to ChatDraft("Good move", ChatDraftRelation.None)))

        assertEquals(ActivityPreviewUiModel(timestamp = 5L, text = "Ben is typing", ongoing = true), row.preview)
    }

    @Test
    fun `an image badge reaches the row as an image`() = runBlocking<Unit> {
        val row = rowOf(chat(activity = null, badge = ChatSummaryBadge.Image("https://example.invalid/badge.png")), drafts = emptyMap())

        assertEquals(ChatListUiState.Badge.Image("https://example.invalid/badge.png"), row.badge)
    }

    private suspend fun rowOf(chat: Chat, drafts: Map<ChatId, ChatDraft>): ChatListUiState.ChatItem {
        val model = ChatListViewModel(FakeListInteractor(chat, drafts), mockk(relaxed = true), mockk(relaxed = true))
        models += model
        val state = withTimeout(5_000) { model.state.first { it is LoadingState.Loaded } } as LoadingState.Loaded
        return state.data.chats.single()
    }

    private fun chat(activity: ChatRoomActivityModel?, badge: ChatSummaryBadge) = Chat(
        id = room,
        display = ChatDisplay("Chess Club", ChatAvatar.Account("Chess Club", themeSeed = byteArrayOf(1))),
        preview = ChatPreview.Message(
            ChatMessage(
                id = "m1",
                chatId = room,
                timestamp = 5L,
                origin = ChatMessageOrigin.Extension("ProductBot_chess"),
                content = ChatMessage.Content.Text("Your move"),
                status = ChatMessage.Status.IS_READ,
            ),
        ),
        timestamp = 5L,
        unreadBadge = badge,
        hasUnseenReaction = false,
        activity = activity,
        customPreviewRenderer = null,
    )

    // ChatListInteractor.subscribeChats takes a context parameter, which mockk cannot stub.
    private class FakeListInteractor(private val chat: Chat, private val drafts: Map<ChatId, ChatDraft>) : ChatListInteractor {
        context(scope: ComputationalScope)
        override fun subscribeChats(): Flow<List<Chat>> = flowOf(listOf(chat))

        context(scope: ComputationalScope)
        override fun subscribeCallSignaling(): Flow<List<ChatMessage>> = flowOf(emptyList())

        override fun subscribeActiveCall(): Flow<ActiveCallState?> = flowOf(null)

        override fun subscribeAllCustomMessageRenderers(): Flow<CustomChatMessageRenderersById> = flowOf(emptyMap())

        override fun subscribePendingIncomingRequestsCount(): Flow<Int> = flowOf(0)

        override suspend fun handleChatOpen(chatId: ChatId): Boolean = false

        override fun subscribeDrafts(): Flow<Map<ChatId, ChatDraft>> = flowOf(drafts)
    }
}
