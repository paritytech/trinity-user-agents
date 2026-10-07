package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatCustomMessage
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.HostRejection
import kotlin.time.Duration.Companion.seconds

class TrUAPIChatSurfacesTest {
    private val sent = mutableListOf<Pair<ProductChatIdParameter, ProductBotMessage>>()

    // Hand-written: mockk hands a value-class argument such as ProductChatIdParameter back unboxed.
    private val messaging = object : ProductChatMessaging {
        override suspend fun createRoom(request: CreateProductRoomRequest) = Result.success(CreateProductRoomResult(CreateRoomStatus.New))

        override suspend fun sendMessage(chatIdParameter: ProductChatIdParameter, message: ProductBotMessage): Result<String> {
            sent += chatIdParameter to message
            return Result.success("message-1")
        }

        override fun subscribeChatRooms() = flowOf(listOf(ProductChatRoom(roomId = "counter", participatingAs = "RoomHost")))
    }

    private val surfaces = TrUAPIChatSurfaces()
    private val bridge = surfaces.bridgeFor(PRODUCT)

    // A Pocket card can boot the shared worker before the product's chat starts, and the worker
    // makes its chat calls once, on start: refusing them would leave the chat empty for its life.
    @Test
    fun `a call made before the chat binds waits for it`() = runTest {
        val created = async { bridge.createRoom("counter", "Counter", "") }
        advanceTimeBy(5.seconds)
        assertFalse(created.isCompleted)

        surfaces.bind(PRODUCT, messaging)

        assertEquals(ChatRoomRegistrationStatus.NEW, created.await())
    }

    // The execution keeps its bridge while the worker runs; a worker that outlives its chat, or
    // whose chat never starts, must not write into rooms nobody is serving.
    @Test
    fun `a product whose chat is not bound has every chat call refused`() = runTest {
        assertRejected { bridge.createRoom("counter", "Counter", "") }

        surfaces.bind(PRODUCT, messaging)
        surfaces.unbind(PRODUCT, messaging)

        assertRejected { bridge.postMessage("counter", ChatMessageContent.Text("hi")) }
        assertRejected { bridge.listRooms() }
    }

    @Test
    fun `releasing a stale binding leaves the newer one serving`() = runTest {
        val newer = mockk<ProductChatMessaging> {
            coEvery { createRoom(any()) } returns Result.success(CreateProductRoomResult(CreateRoomStatus.Exists))
        }
        surfaces.bind(PRODUCT, messaging)
        surfaces.bind(PRODUCT, newer)

        surfaces.unbind(PRODUCT, messaging)

        assertEquals(ChatRoomRegistrationStatus.EXISTS, bridge.createRoom("counter", "Counter", ""))
    }

    // The returned id is what a render context names the message by, so it must be the stored one.
    @Test
    fun `text and custom bodies land in the room and answer with the stored id`() = runTest {
        surfaces.bind(PRODUCT, messaging)
        val payload = byteArrayOf(1, 2, 3)

        val textId = bridge.postMessage("counter", ChatMessageContent.Text("hello"))
        val customId = bridge.postMessage("counter", ChatMessageContent.Custom(ChatCustomMessage("counter/v1", payload)))

        assertEquals(listOf("message-1", "message-1"), listOf(textId, customId))
        assertEquals(ProductChatIdParameter("counter"), sent[0].first)
        assertEquals(ProductBotMessage.Text("hello"), sent[0].second)
        val custom = sent[1].second as ProductBotMessage.Custom
        assertEquals("counter/v1", custom.messageType)
        assertArrayEquals(payload, custom.data.value)
    }

    @Test
    fun `bodies without a native surface, roomless bodies and bots are refused`() = runTest {
        surfaces.bind(PRODUCT, messaging)

        assertRejected { bridge.postMessage("counter", ChatMessageContent.RichText(mockk())) }
        assertRejected { bridge.postMessage(" ", ChatMessageContent.Text("hi")) }
        assertRejected { bridge.createRoom("", "Counter", "") }
        assertRejected { bridge.registerBot("counter-bot", "Counter", "") }
        assertTrue(sent.isEmpty())
    }

    @Test
    fun `rooms are listed as hosted by the product`() = runTest {
        surfaces.bind(PRODUCT, messaging)

        assertEquals(listOf(ChatRoom("counter", ChatRoomParticipation.ROOM_HOST)), bridge.listRooms())
    }

    private suspend fun assertRejected(call: suspend () -> Unit) {
        val outcome = runCatching { call() }
        assertTrue("expected a rejection, got $outcome", outcome.exceptionOrNull() is HostRejection)
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("counter.paseo")
    }
}
