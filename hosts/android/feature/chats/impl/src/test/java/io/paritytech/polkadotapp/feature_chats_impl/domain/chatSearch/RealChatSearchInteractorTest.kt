package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatMessageSearchHit
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.search.CompoundChatSearchResultProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.StartChatDataUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SubscribeActiveChatsUseCase
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class RealChatSearchInteractorTest {
    private val subscribeActiveChats: SubscribeActiveChatsUseCase = mock()
    private val chatMessageRepository: ChatMessageRepository = mock()
    private val compoundChatSearchResultProvider: CompoundChatSearchResultProvider = mock()
    private val startChatDataUseCase: StartChatDataUseCase = mock()
    private val chatSearchRecentsRepository: ChatSearchRecentsRepository = mock()

    private val sut = RealChatSearchInteractor(
        subscribeActiveChats = subscribeActiveChats,
        chatMessageRepository = chatMessageRepository,
        compoundChatSearchResultProvider = compoundChatSearchResultProvider,
        startChatDataUseCase = startChatDataUseCase,
        chatSearchRecentsRepository = chatSearchRecentsRepository,
    )

    // region — search() pipeline (mocked sources) ---------------------------------------------

    @Test
    fun `merges existing chat, message and app results when all sources succeed`() = runBlocking<Unit> {
        val knownChat = chat(name = "alice.01")
        withMessages(Result.success(listOf(messageHit(snippet = "let's meet", chatId = knownChat.id))))
        withApps(listOf(appResult("market.dot")))

        val results = search("ali", chats = listOf(knownChat)).assertSuccess()

        assertTrue(results.any { it is ChatListSearchResult.Chat })
        assertTrue(results.any { it is ChatListSearchResult.Message })
        assertTrue(results.any { it is ChatListSearchResult.App })
    }

    @Test
    fun `message hits without a matching chat in the list are dropped`() = runBlocking<Unit> {
        withMessages(Result.success(listOf(messageHit(snippet = "hello there"))))
        withApps(emptyList())

        val results = search("hello").assertSuccess()

        assertEquals(0, results.size)
    }

    @Test
    fun `message hit is kept and titled with its chat when that chat is passed in`() = runBlocking<Unit> {
        val knownChat = chat(name = "alice.01")
        withMessages(Result.success(listOf(messageHit(snippet = "let's meet", chatId = knownChat.id))))
        withApps(emptyList())

        val results = search("meet", chats = listOf(knownChat)).assertSuccess()

        val message = results.filterIsInstance<ChatListSearchResult.Message>().single()
        assertEquals("alice.01", message.title)
    }

    @Test
    fun `message search failure fails the whole search so the UI can tell it from no results`() = runBlocking<Unit> {
        withMessages(Result.failure(RuntimeException("message search down")))
        withApps(listOf(appResult("market.dot")))

        val result = search("a")

        assertTrue("expected Result.failure but was ${result.getOrNull()}", result.isFailure)
        assertEquals("message search down", result.exceptionOrNull()?.message)
    }

    // endregion

    // region — mergeChatSearchResults (pure) --------------------------------------------------

    @Test
    fun `merge includes existing chats matching the query by name`() {
        val results = mergeChatSearchResults(
            query = "ali",
            chats = listOf(chat(name = "alice.01"), chat(name = "bob.02")),
            messageHits = emptyList(),
            apps = emptyList(),
        )

        val chatResult = results.filterIsInstance<ChatListSearchResult.Chat>().single()
        assertEquals("alice.01", chatResult.title)
    }

    @Test
    fun `merge titles a message hit with its chat name when the chat is known`() {
        val knownChat = chat(name = "alice.01")

        val results = mergeChatSearchResults(
            query = "meet",
            chats = listOf(knownChat),
            messageHits = listOf(messageHit(snippet = "let's meet tomorrow", chatId = knownChat.id)),
            apps = emptyList(),
        )

        val message = results.filterIsInstance<ChatListSearchResult.Message>().single()
        assertEquals("alice.01", message.title)
        assertEquals("let's meet tomorrow", message.snippet)
    }

    @Test
    fun `merge drops a message hit whose chat is not in the list`() {
        val results = mergeChatSearchResults(
            query = "meet",
            chats = listOf(chat(name = "alice.01")),
            messageHits = listOf(messageHit(snippet = "let's meet tomorrow")),
            apps = emptyList(),
        )

        assertTrue(results.filterIsInstance<ChatListSearchResult.Message>().isEmpty())
    }

    // endregion

    // region — test harness ------------------------------------------------------------------

    private fun search(
        query: String,
        chats: List<Chat> = emptyList(),
    ): Result<List<ChatListSearchResult>> = runBlocking {
        sut.search(query, chats)
    }

    private fun Result<List<ChatListSearchResult>>.assertSuccess(): List<ChatListSearchResult> {
        assertTrue("expected Result.success but was ${exceptionOrNull()}", isSuccess)
        return getOrThrow()
    }

    private suspend fun withMessages(result: Result<List<ChatMessageSearchHit>>) {
        whenever(chatMessageRepository.searchMessages(any(), any())).thenReturn(result)
    }

    private suspend fun withApps(apps: List<ChatListSearchResult.App>) {
        whenever(compoundChatSearchResultProvider.search(any())).thenReturn(apps)
    }

    private fun appResult(id: String) = ChatListSearchResult.App(
        id = id,
        title = "Market",
        providerId = "TestProvider",
    )

    private fun messageHit(
        snippet: String,
        chatId: ChatId = ChatId.fromRawValue(byteArrayOf(0x02)),
    ): ChatMessageSearchHit {
        return ChatMessageSearchHit(
            chatId = chatId,
            messageId = "message-1",
            snippet = snippet,
            timestamp = 0L,
        )
    }

    private fun chat(
        name: String,
        chatId: ChatId = ChatId.fromRawValue(name.encodeToByteArray()),
    ): Chat {
        return Chat(
            id = chatId,
            display = ChatDisplay(name, ChatAvatar.Account(name, name.encodeToByteArray())),
            preview = ChatPreview.EmptyChat,
            timestamp = 0L,
            unreadBadge = ChatSummaryBadge.None,
            hasUnseenReaction = false,
            customPreviewRenderer = null,
        )
    }

    // endregion
}
