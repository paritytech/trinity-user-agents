package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCacheTest
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductRoomCacheEntry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RealProductFeedListenersTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")
    private val otherRoom = ChatId.forExtensionRoom("ProductBot_game.dot", "hall")
    private val update = ProductFeedUpdateModel.StatusChanged("m1", ProductMessageStatusModel.READ)
    private val cache = ProductMessageCache()
    private val listeners = RealProductFeedListeners(cache)

    @Test
    fun `an update goes to each screen of its room only`() = runBlocking<Unit> {
        val received = mutableListOf<Pair<ChatId, ProductFeedUpdateModel>>()
        listeners.register(room) { chatId, update -> received += chatId to update }
        listeners.register(otherRoom) { _, _ -> error("another room must not take it") }

        val delivered = listeners.deliver(room, update)

        assertTrue(delivered)
        assertEquals(listOf(room to update), received)
    }

    @Test
    fun `an update for a room that no screen shows goes nowhere`() = runBlocking<Unit> {
        assertFalse(listeners.deliver(room, update))
    }

    @Test
    fun `an update for a room that no screen shows changes its cached page`() = runBlocking<Unit> {
        cache.store(room, ProductRoomCacheEntry(emptyList(), next = null, seenMessageId = null, seenTimestamp = null, reportedSeenMessageId = null))
        val screen = mutableListOf<ProductFeedUpdateModel>()
        val registration = listeners.register(room) { _, update -> screen += update }
        val first = ProductFeedUpdateModel.New(ProductMessageCacheTest.message("m1", 1))
        val second = ProductFeedUpdateModel.New(ProductMessageCacheTest.message("m2", 2))

        listeners.deliver(room, first)
        registration.cancel()
        listeners.deliver(room, second)

        assertEquals(listOf(first), screen)
        assertEquals(listOf("m2"), cache.entry(room)?.messages?.map { it.messageId })
    }

    @Test
    fun `a cancelled registration takes no more updates`() = runBlocking<Unit> {
        val registration = listeners.register(room) { _, _ -> error("cancelled") }

        registration.cancel()
        registration.cancel()

        assertFalse(listeners.deliver(room, update))
    }
}
