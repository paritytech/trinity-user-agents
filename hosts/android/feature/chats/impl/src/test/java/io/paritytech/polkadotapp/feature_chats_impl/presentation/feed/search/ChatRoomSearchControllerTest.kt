package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.search

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.ChatRoomSearchInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchAuthor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchCursor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchHit
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RoomSearchPage
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChatRoomSearchControllerTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val policy = MutableStateFlow<ChatRoomPolicy?>(ChatRoomPolicy.Defaults)
    private val interactor = mockk<ChatRoomSearchInteractor>()
    private val jumps = mutableListOf<String>()

    private val productSource = ChatRoomPolicy.Defaults.copy(
        messageStore = ChatRoomPolicy.MessageStore.PRODUCT,
        features = ChatRoomFeaturesModel.HOST_DEFAULT.copy(search = true),
    )

    @Test
    fun `the host asks once the user stops typing for 300 ms and shows author, time and match`() = runTest {
        coEvery { interactor.search(room, ChatRoomPolicy.Defaults, "creme", null) } returns Result.success(
            RoomSearchPage.Hits(listOf(hit("m2", RoomSearchAuthor.User), hit("m1", RoomSearchAuthor.Room)), next = null),
        )
        val search = controller()

        search.handle(RoomSearchAction.Open)
        "creme".indices.forEach { end ->
            search.handle(RoomSearchAction.QueryChanged("creme".substring(0, end + 1)))
            advanceTimeBy(100)
        }
        advanceTimeBy(250)
        runCurrent()

        coVerify(exactly = 1) { interactor.search(any(), any(), any(), any()) }
        val results = search.uiState.value.results as ChatRoomSearchUiState.Results.Hits
        assertEquals(listOf("m2", "m1"), results.items.map { it.messageId })
        assertEquals(RoomSearchResultUiModel.Author.You, results.items[0].author)
        assertEquals(RoomSearchResultUiModel.Author.Named("Sample"), results.items[1].author)
        assertEquals(listOf(4..8), results.items[0].excerpt.highlights)
        assertFalse(results.canLoadMore)
    }

    @Test
    fun `an older page comes after the newest one, and a tap jumps to the message and closes the field`() = runTest {
        val cursor = RoomSearchCursor.HostStore(timestamp = 5, messageId = "m5")
        coEvery { interactor.search(room, any(), "x", null) } returns Result.success(RoomSearchPage.Hits(listOf(hit("m9")), cursor))
        coEvery { interactor.search(room, any(), "x", cursor) } returns Result.success(RoomSearchPage.Hits(listOf(hit("m4")), next = null))
        val search = controller()

        search.handle(RoomSearchAction.Open)
        search.handle(RoomSearchAction.QueryChanged("x"))
        advanceTimeBy(400)
        assertTrue((search.uiState.value.results as ChatRoomSearchUiState.Results.Hits).canLoadMore)

        search.handle(RoomSearchAction.LoadMore)
        runCurrent()
        val results = search.uiState.value.results as ChatRoomSearchUiState.Results.Hits
        assertEquals(listOf("m9", "m4"), results.items.map { it.messageId })

        search.handle(RoomSearchAction.Select("m4"))
        runCurrent()
        assertEquals(listOf("m4"), jumps)
        assertFalse(search.uiState.value.isActive)
        assertEquals(ChatRoomSearchUiState.Results.Idle, search.uiState.value.results)
    }

    @Test
    fun `a product that does not search shows not available`() = runTest {
        policy.value = productSource
        coEvery { interactor.search(room, productSource, "x", null) } returns Result.success(RoomSearchPage.NotAvailable)
        val search = controller(served = listOf(ChatHostCapabilities.SEARCH))

        search.handle(RoomSearchAction.Open)
        search.handle(RoomSearchAction.QueryChanged("x"))
        advanceTimeBy(400)

        assertEquals(ChatRoomSearchUiState.Results.NotAvailable, search.uiState.value.results)
    }

    @Test
    fun `a product-source room without search on both sides has no search action`() = runTest {
        policy.value = productSource
        val search = controller(served = emptyList())

        search.handle(RoomSearchAction.Open)
        runCurrent()

        assertFalse(search.uiState.value.isAvailable)
        assertFalse(search.uiState.value.isActive)
    }

    private fun TestScope.controller(served: Collection<String> = emptyList()): ChatRoomSearchController {
        val controller = ChatRoomSearchController(
            scope = backgroundScope,
            chatId = room,
            roomPolicy = policy,
            roomName = flowOf("Sample"),
            interactor = interactor,
            servedFeatures = served,
            onJump = { jumps += it },
        )
        runCurrent()
        return controller
    }

    private fun hit(id: String, author: RoomSearchAuthor = RoomSearchAuthor.Room) =
        RoomSearchHit(messageId = id, timestamp = 1, text = "Une crème $id", author = author)
}
