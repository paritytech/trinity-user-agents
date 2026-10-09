package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.inOrder
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomParticipation
import java.util.concurrent.atomic.AtomicInteger

private const val ROOM_1 = "room-1"
private const val ROOM_2 = "room-2"

class TrUAPIChatRoomForwardingTest {
    private val productId = ProductId.fromStoredValue("chat.dot")
    private val execution: TrUAPIProductExecution = mock()

    private class Forwarding(val scope: CoroutineScope, val job: Job)

    private fun TestScope.forwarding(chatMessaging: FakeChatMessaging): Forwarding {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val job = TrUAPIChatRoomForwarding(productId, execution, chatMessaging).start(scope)
        return Forwarding(scope, job)
    }

    private fun emissions() = MutableSharedFlow<List<ProductChatRoom>>(extraBufferCapacity = 1)

    @Test
    fun `each room list emission reaches notifyChatRoomsChanged in order`() = runTest {
        val rooms = emissions()
        forwarding(FakeChatMessaging(rooms = rooms))
        runCurrent()

        rooms.tryEmit(listOf(ProductChatRoom(ROOM_1, RoomParticipation.ROOM_HOST)))
        runCurrent()
        rooms.tryEmit(listOf(ProductChatRoom(ROOM_1, RoomParticipation.ROOM_HOST), ProductChatRoom(ROOM_2, RoomParticipation.BOT)))
        runCurrent()

        val order = inOrder(execution)
        order.verify(execution).notifyChatRoomsChanged(listOf(ChatRoom(ROOM_1, ChatRoomParticipation.ROOM_HOST)))
        order.verify(execution).notifyChatRoomsChanged(
            listOf(
                ChatRoom(ROOM_1, ChatRoomParticipation.ROOM_HOST),
                ChatRoom(ROOM_2, ChatRoomParticipation.BOT),
            ),
        )
    }

    @Test
    fun `a bound surface with genuinely zero rooms pushes an empty list`() = runTest {
        forwarding(FakeChatMessaging(rooms = flowOf(emptyList())))
        runCurrent()

        verify(execution, times(1)).notifyChatRoomsChanged(emptyList())
    }

    @Test
    fun `forwarding stops when the worker's scope is cancelled`() = runTest {
        val rooms = emissions()
        val forwarding = forwarding(FakeChatMessaging(rooms = rooms))
        runCurrent()

        rooms.tryEmit(listOf(ProductChatRoom(ROOM_1, RoomParticipation.ROOM_HOST)))
        runCurrent()
        verify(execution, times(1)).notifyChatRoomsChanged(any())

        forwarding.scope.cancel()
        runCurrent()

        rooms.tryEmit(listOf(ProductChatRoom(ROOM_2, RoomParticipation.BOT)))
        runCurrent()

        verify(execution, times(1)).notifyChatRoomsChanged(any())
        verify(execution, never()).notifyChatRoomsChanged(listOf(ChatRoom(ROOM_2, ChatRoomParticipation.BOT)))
    }

    @Test
    fun `each upstream failure waits longer than the last`() = runTest {
        val subscriptions = AtomicInteger(0)
        val rooms = flow {
            if (subscriptions.incrementAndGet() <= 2) throw IllegalStateException("rooms unavailable")
            emit(listOf(ProductChatRoom(ROOM_1, RoomParticipation.ROOM_HOST)))
        }
        forwarding(FakeChatMessaging(rooms = rooms))

        advanceTimeBy(1_100)
        assertEquals("the first retry waits a second", 2, subscriptions.get())
        advanceTimeBy(1_100)
        assertEquals("the second waits two, not one", 2, subscriptions.get())
        advanceTimeBy(1_000)
        assertEquals(3, subscriptions.get())
    }

    @Test
    fun `an upstream failure resubscribes and forwarding resumes`() = runTest {
        val subscriptions = AtomicInteger(0)
        val rooms = flow {
            if (subscriptions.incrementAndGet() == 1) throw IllegalStateException("rooms unavailable")
            emit(listOf(ProductChatRoom(ROOM_1, RoomParticipation.ROOM_HOST)))
        }
        forwarding(FakeChatMessaging(rooms = rooms))
        advanceUntilIdle()

        assertEquals(2, subscriptions.get())
        verify(execution).notifyChatRoomsChanged(listOf(ChatRoom(ROOM_1, ChatRoomParticipation.ROOM_HOST)))
    }

    @Test
    fun `a push that throws is dropped without resubscribing`() = runTest {
        val rooms = emissions()
        val subscriptions = AtomicInteger(0)
        whenever(execution.notifyChatRoomsChanged(any()))
            .thenThrow(IllegalStateException("object has already been destroyed"))
            .thenAnswer { }
        val forwarding = forwarding(FakeChatMessaging(rooms = rooms.onStart { subscriptions.incrementAndGet() }))
        runCurrent()

        rooms.tryEmit(listOf(ProductChatRoom(ROOM_1, RoomParticipation.ROOM_HOST)))
        runCurrent()
        rooms.tryEmit(listOf(ProductChatRoom(ROOM_2, RoomParticipation.ROOM_HOST)))
        runCurrent()

        assertTrue(forwarding.job.isActive)
        assertEquals(1, subscriptions.get())
        verify(execution).notifyChatRoomsChanged(listOf(ChatRoom(ROOM_2, ChatRoomParticipation.ROOM_HOST)))
    }
}
