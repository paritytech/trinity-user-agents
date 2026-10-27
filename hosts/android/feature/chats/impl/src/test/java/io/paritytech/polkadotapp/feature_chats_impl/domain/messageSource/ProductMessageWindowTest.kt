package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessagesPage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The pages of a product-source room that the room screen shows. */
class ProductMessageWindowTest {
    private val extensionId = "ProductBot_weather"

    @Test
    fun `the newest page opens the window and its cursor asks the page before it`() {
        val window = ProductMessageWindow(windowLimit = 10)

        window.replaceWith(ProductMessagesPage((3..5).map { productMessage(it) }, next = "3"))

        assertEquals(listOf("m3", "m4", "m5"), window.orderedIds)
        assertEquals(ChatPageAnchorModel.Before("3"), window.olderAnchor)
        assertTrue(window.isAtNewestEnd)
    }

    @Test
    fun `an older page past the limit drops the newest pages and keeps their requests`() {
        val window = ProductMessageWindow(windowLimit = 3)
        window.replaceWith(ProductMessagesPage((3..5).map { productMessage(it) }, next = "3"))

        val added = window.addOlder(ChatPageAnchorModel.Before("3"), ProductMessagesPage((0..2).map { productMessage(it) }, next = null))

        assertEquals(3, added)
        assertEquals(listOf("m0", "m1", "m2"), window.orderedIds)
        assertEquals(listOf<ChatPageAnchorModel>(ChatPageAnchorModel.Before(null)), window.droppedNewer)
        assertFalse(window.isAtNewestEnd)
        assertNull(window.olderAnchor)
    }

    @Test
    fun `a live message goes after the newest message only while the newest page is in the window`() {
        val window = ProductMessageWindow(windowLimit = 10)
        window.replaceWith(ProductMessagesPage(listOf(productMessage(1)), next = null))

        window.insertLive(productMessage(2))

        assertEquals(listOf("m1", "m2"), window.orderedIds)
    }

    @Test
    fun `the snapshot is newest first, and an incoming message is unread until the product reads it`() {
        val window = ProductMessageWindow(windowLimit = 10)
        window.replaceWith(
            ProductMessagesPage(
                listOf(
                    productMessage(1, status = ProductMessageStatusModel.READ),
                    productMessage(2),
                    productMessage(3, participant = ProductParticipant.USER),
                ),
                next = null,
            ),
        )

        val snapshot = window.snapshot(productRoom, extensionId, ChatPagingState.Complete, ChatRoomReadiness.READY)

        assertEquals(listOf("m3", "m2", "m1"), snapshot.messages.map { it.id })
        assertEquals(ChatMessage.Status.IS_SENT, snapshot.messages[0].status)
        assertEquals(ChatMessage.Status.NEW, snapshot.messages[1].status)
        assertEquals(ChatMessage.Status.IS_READ, snapshot.messages[2].status)
    }

    @Test
    fun `the cache keeps the newest page of a window at its newest end`() {
        val window = ProductMessageWindow(windowLimit = 10)
        window.replaceWith(ProductMessagesPage((1..3).map { productMessage(it) }, next = "1"))

        val entry = requireNotNull(window.cacheEntry(pageSize = 2))

        assertEquals(listOf("m2", "m3"), entry.messages.map { it.messageId })
        assertNull(entry.next)
    }
}
