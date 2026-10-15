package io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatHostCapabilities
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductSearchPage
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RealChatRoomSearchInteractorTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "source")
    private val querying = mockk<ProductChatQuerying>()
    private val engine = mockk<ChatEngine> { coEvery { roomQuerying(room) } returns querying }
    private val store = FakeCandidates(emptyList())
    private val interactor = RealChatRoomSearchInteractor(store, engine)

    private val productSource = ChatRoomPolicy.Defaults.copy(
        messageStore = ChatRoomPolicy.MessageStore.PRODUCT,
        features = ChatRoomFeaturesModel.HOST_DEFAULT.copy(search = true),
    )

    @Test
    fun `a product-source room asks search_query with the text, the cursor and a page of 30`() = runBlocking<Unit> {
        coEvery { querying.searchPage(room, "crème", "c1", 30) } returns Result.success(
            ProductSearchPage(listOf(hit("m9", "user"), hit("m7", "p1"), hit("m5", "product")), next = "c2"),
        )

        val page = interactor.search(room, productSource, " crème ", RoomSearchCursor.Product("c1")).getOrThrow() as RoomSearchPage.Hits

        assertEquals(listOf("m9", "m7", "m5"), page.hits.map { it.messageId })
        assertEquals(
            listOf(RoomSearchAuthor.User, RoomSearchAuthor.Participant("p1"), RoomSearchAuthor.Room),
            page.hits.map { it.author },
        )
        assertEquals("Hit m9", page.hits.first().text)
        assertEquals(RoomSearchCursor.Product("c2"), page.next)
    }

    @Test
    fun `NotAvailable or no answer in time leaves the room without search results`() = runBlocking<Unit> {
        coEvery { querying.searchPage(any(), any(), any(), any()) } returns Result.failure(ProductChatQueryException.NotAvailable())
        assertEquals(RoomSearchPage.NotAvailable, interactor.search(room, productSource, "x", null).getOrThrow())

        coEvery { querying.searchPage(any(), any(), any(), any()) } returns Result.failure(ProductChatQueryException.TimedOut())
        assertEquals(RoomSearchPage.NotAvailable, interactor.search(room, productSource, "x", null).getOrThrow())
    }

    @Test
    fun `a room that the host keeps never asks the product`() = runBlocking<Unit> {
        val page = interactor.search(room, ChatRoomPolicy.Defaults, "x", null).getOrThrow()

        assertEquals(RoomSearchPage.Hits(emptyList(), next = null), page)
        coVerify(exactly = 0) { querying.searchPage(any(), any(), any(), any()) }
    }

    @Test
    fun `the header offers the search in a host-store room, and in a product-source room only with search on both sides`() {
        val served = listOf(ChatHostCapabilities.SEARCH)

        assertTrue(ChatRoomPolicy.Defaults.offersRoomSearch(emptyList()))
        assertTrue(productSource.offersRoomSearch(served))
        assertFalse(productSource.offersRoomSearch(emptyList()))
        assertFalse(productSource.copy(features = ChatRoomFeaturesModel.HOST_DEFAULT).offersRoomSearch(served))
        assertFalse((null as ChatRoomPolicy?).offersRoomSearch(served))
    }

    private fun hit(id: String, participantId: String) = ProductRoomMessage(
        messageId = id,
        participantId = participantId,
        timestamp = 1,
        content = ChatMessage.Content.Text("Hit $id"),
        status = ProductMessageStatusModel.SENT,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
    )
}
