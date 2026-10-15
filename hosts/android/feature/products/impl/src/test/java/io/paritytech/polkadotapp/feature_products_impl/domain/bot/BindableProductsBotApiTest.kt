package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class BindableProductsBotApiTest {
    private val productId = ProductId.fromStoredValue("coinflip.dot")
    private val hostApiInteractor: HostApiInteractor = mock()
    private val chatIdParam = ProductChatIdParameter("room")

    private fun bindable() = BindableProductsBotApi(hostApiInteractor, FixedProductId(productId))

    @Test
    fun `outgoing chat calls fail with messaging-not-supported when unbound`() = runTest {
        val api = bindable()

        val result = api.sendMessage(chatIdParam, ProductBotMessage.Text("hi"))

        assertTrue(result.isFailure)
        assertEquals("messaging_not_supported", (result.exceptionOrNull() as? HostCallException)?.code)
    }

    @Test
    fun `outgoing chat calls route to the bound target`() = runTest {
        val api = bindable()
        api.chatSlot.set(FakeChatMessaging(onSendMessage = { _, _ -> Result.success("m1") }))

        val result = api.sendMessage(chatIdParam, ProductBotMessage.Text("hi"))

        assertEquals("m1", result.getOrThrow())
    }

    @Test
    fun `unbind restores the messaging-not-supported behaviour`() = runTest {
        val api = bindable()
        val messaging = FakeChatMessaging(onSendMessage = { _, _ -> Result.success("m1") })
        api.chatSlot.set(messaging)
        api.chatSlot.clear(messaging)

        assertTrue(api.sendMessage(chatIdParam, ProductBotMessage.Text("hi")).isFailure)
    }

    @Test
    fun `an unbound surface reports no rooms instead of never answering`() = runTest {
        val api = bindable()

        assertEquals(emptyList<ProductChatRoom>(), api.subscribeChatRooms().first())
    }

    @Test
    fun `room subscription follows a binding that arrives after the subscriber`() = runTest {
        val api = bindable()
        val rooms = mutableListOf<List<ProductChatRoom>>()
        val collector = launch { api.subscribeChatRooms().toList(rooms) }
        runCurrent()

        val messaging = FakeChatMessaging(rooms = flowOf(listOf(ProductChatRoom("room-1", RoomParticipation.ROOM_HOST))))
        api.chatSlot.set(messaging)
        runCurrent()

        assertEquals(listOf(emptyList(), listOf("room-1")), rooms.map { list -> list.map { it.roomId } })
        collector.cancel()
    }

    @Test
    fun `a stale release leaves a newer binding alone`() = runTest {
        val api = bindable()
        val replaced = FakeChatMessaging(rooms = flowOf(listOf(ProductChatRoom("room-1", RoomParticipation.ROOM_HOST))))
        val current = FakeChatMessaging(rooms = flowOf(listOf(ProductChatRoom("room-2", RoomParticipation.ROOM_HOST))))
        api.chatSlot.set(replaced)
        api.chatSlot.set(current)

        // The replaced extension disposes after the new one has bound; it must not take it with it.
        api.chatSlot.clear(replaced)

        assertEquals(listOf(listOf("room-2")), listOf(api.subscribeChatRooms().first().map { it.roomId }))
    }

    @Test
    fun `releasing the binding reports no rooms and ignores the released surface`() = runTest {
        val api = bindable()
        val bound = MutableStateFlow(listOf(ProductChatRoom("room-1", RoomParticipation.ROOM_HOST)))
        val messaging = FakeChatMessaging(rooms = bound)
        api.chatSlot.set(messaging)
        val rooms = mutableListOf<List<ProductChatRoom>>()
        val collector = launch { api.subscribeChatRooms().toList(rooms) }
        runCurrent()

        api.chatSlot.clear(messaging)
        runCurrent()
        bound.value = listOf(ProductChatRoom("room-2", RoomParticipation.BOT))
        runCurrent()

        assertEquals(listOf(listOf("room-1"), emptyList()), rooms.map { list -> list.map { it.roomId } })
        assertTrue(collector.isActive)
        collector.cancel()
    }

    @Test
    fun `room subscription switches to a replacement binding`() = runTest {
        val api = bindable()
        val first = FakeChatMessaging(rooms = flowOf(listOf(ProductChatRoom("room-1", RoomParticipation.ROOM_HOST))))
        api.chatSlot.set(first)
        val rooms = mutableListOf<List<ProductChatRoom>>()
        val collector = launch { api.subscribeChatRooms().toList(rooms) }
        runCurrent()

        val second = FakeChatMessaging(rooms = flowOf(listOf(ProductChatRoom("room-2", RoomParticipation.BOT))))
        api.chatSlot.set(second)
        runCurrent()

        assertEquals(listOf(listOf("room-1"), listOf("room-2")), rooms.map { list -> list.map { it.roomId } })
        collector.cancel()
    }
}
