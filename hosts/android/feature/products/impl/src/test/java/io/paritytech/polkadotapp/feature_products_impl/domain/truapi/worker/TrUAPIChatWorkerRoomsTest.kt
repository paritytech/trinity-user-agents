package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.ChatQueryException
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatEvent
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatPageAnchor
import uniffi.truapi.ChatUserAction
import uniffi.truapi.ProductChatMessagesQueryItem
import uniffi.truapi.ProductChatMessagesQueryRequest
import uniffi.truapi.ProductChatQueryError
import uniffi.truapi.ProductChatUserActionItem
import kotlin.time.Duration.Companion.seconds

class TrUAPIChatWorkerRoomsTest {
    private val productId = ProductId.fromStoredValue("game.dot")
    private val room = ProductChatIdParameter("lobby")
    private val execution: TrUAPIProductExecution = mockk(relaxed = true)
    private val runtime: TrUAPIHostRuntime = mockk(relaxed = true)

    private fun TestScope.worker(scope: CoroutineScope = CoroutineScope(StandardTestDispatcher(testScheduler))): TrUAPIChatWorker {
        val workers: TrUAPIWorkerSupervisor = mockk()
        justRun { workers.restartIfFailed(productId) }
        every { workers.executionState(productId) } returns flowOf(WorkerExecutionState.Running(execution))
        every { workers.currentExecution(productId) } returns execution
        return TrUAPIChatWorker(productId, runtime, workers, FakeChatMessaging(), scope)
    }

    @Test
    fun `an open room holds the worker and tells the product until it closes`() = runTest {
        val worker = worker()

        assertTrue(worker.openRoom(room).isSuccess)
        advanceUntilIdle()
        worker.closeRoom(room)

        verify(exactly = 1) { runtime.acquireWorker(productId.value) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.RoomOpened) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.RoomClosed) }
        verify(exactly = 1) { runtime.releaseWorker(productId.value) }
    }

    @Test
    fun `opening a room asks the supervisor to start a worker whose start failed`() = runTest {
        val workers: TrUAPIWorkerSupervisor = mockk(relaxed = true)
        every { workers.executionState(productId) } returns flowOf(WorkerExecutionState.Running(execution))
        val worker = TrUAPIChatWorker(productId, runtime, workers, FakeChatMessaging(), CoroutineScope(StandardTestDispatcher(testScheduler)))

        worker.openRoom(room)

        verify { workers.restartIfFailed(productId) }
    }

    @Test
    fun `closing a room that never opened sends nothing`() = runTest {
        worker().closeRoom(room)

        verify(exactly = 0) { execution.publishChatEvent(any(), any(), ChatEvent.RoomClosed) }
        verify(exactly = 0) { runtime.releaseWorker(any()) }
    }

    @Test
    fun `a worker that cannot be held fails the open`() = runTest {
        every { runtime.acquireWorker(productId.value) } throws IllegalStateException("runtime closed")

        assertTrue(worker().openRoom(room).isFailure)
    }

    @Test
    fun `a disposed chat gives back the rooms that are still open`() = runTest {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val worker = worker(scope)
        worker.openRoom(room)

        scope.cancel()
        advanceUntilIdle()

        verify(exactly = 1) { runtime.releaseWorker(productId.value) }
    }

    @Test
    fun `an event without an open room holds the worker for a short while`() = runTest {
        val worker = worker()

        worker.publishRoomEvent(room, ProductRoomEvent.MessagePosted("h1", ChatMessage.Content.Text("hi"), null))
        runCurrent()
        verify(exactly = 1) { runtime.acquireWorker(productId.value) }
        verify(exactly = 0) { runtime.releaseWorker(any()) }

        advanceTimeBy(31.seconds)

        verify(exactly = 1) { runtime.releaseWorker(productId.value) }
    }

    @Test
    fun `a page of messages is asked with the anchor of the room screen`() = runTest {
        coEvery { execution.chatMessagesQuery(any(), any()) } returns ProductChatMessagesQueryItem(emptyList(), next = "c2")

        val page = worker().messagesPage(room, ChatPageAnchorModel.Before("c1"), pageSize = 40)

        assertEquals("c2", page.getOrNull()?.next)
        coVerify { execution.chatMessagesQuery(ProductChatMessagesQueryRequest("lobby", ChatPageAnchor.Before("c1"), 40u), any()) }
    }

    @Test
    fun `a question the product does not answer hides the feature`() = runTest {
        coEvery { execution.chatMessagesQuery(any(), any()) } throws ChatQueryException.Refused(ProductChatQueryError.NotAvailable)

        val page = worker().messagesPage(room, ChatPageAnchorModel.Before(null), pageSize = 40)

        assertTrue(page.exceptionOrNull() is ProductChatQueryException.NotAvailable)
        coVerify(exactly = 1) { execution.chatMessagesQuery(any(), any()) }
    }

    @Test
    fun `an unavailable worker is asked once more after a second`() = runTest {
        coEvery { execution.chatUserAction(any(), any()) } throws ChatQueryException.Failed("not connected") andThen
            ProductChatUserActionItem.Acknowledged

        val outcome = worker().userAction(room, ChatUserActionModel.Seen("m1"))

        assertEquals(ChatUserActionOutcome.Acknowledged, outcome.getOrNull())
        coVerify(exactly = 2) { execution.chatUserAction(any(), any()) }
    }

    @Test
    fun `a new message goes to the product with its body`() = runTest {
        coEvery { execution.chatUserAction(any(), any()) } returns ProductChatUserActionItem.Acknowledged

        worker().userAction(room, ChatUserActionModel.NewMessage(ChatMessage.Content.Text("hi"), replyTo = "m0"))

        coVerify {
            execution.chatUserAction(
                match { it.roomId == "lobby" && it.action == ChatUserAction.NewMessage(ChatMessageBody.Text("hi"), "m0") },
                any(),
            )
        }
    }

    @Test
    fun `a rejected action carries its reason to the screen`() = runTest {
        coEvery { execution.chatUserAction(any(), any()) } throws
            ChatQueryException.ActionRefused(uniffi.truapi.ProductChatUserActionError.Rejected("too long"))

        val outcome = worker().userAction(room, ChatUserActionModel.DeleteMessage("m1"))

        assertEquals("too long", (outcome.exceptionOrNull() as ProductChatQueryException.Rejected).reason)
    }
}
