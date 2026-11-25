package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RealProductFeedListenersTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")
    private val otherRoom = ChatId.forExtensionRoom("ProductBot_game.dot", "hall")
    private val update = ProductFeedUpdateModel.StatusChanged("m1", ProductMessageStatusModel.READ)
    private val listeners = RealProductFeedListeners()

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
    fun `a cancelled registration takes no more updates`() = runBlocking<Unit> {
        val registration = listeners.register(room) { _, _ -> error("cancelled") }

        registration.cancel()
        registration.cancel()

        assertFalse(listeners.deliver(room, update))
    }
}
