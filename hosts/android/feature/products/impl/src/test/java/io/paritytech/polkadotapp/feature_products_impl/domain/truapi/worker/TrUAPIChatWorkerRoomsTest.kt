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
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatPageAnchorModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatPageAnchor
import uniffi.truapi.ProductChatMessagesQueryItem
import uniffi.truapi.ProductChatMessagesQueryRequest
import uniffi.truapi.ProductChatQueryError
import uniffi.truapi.ProductRuntimeException

class TrUAPIChatWorkerRoomsTest {
    private val productId = ProductId.fromStoredValue("game.dot")
    private val room = ProductChatIdParameter("lobby")
    private val execution: TrUAPIProductExecution = mockk(relaxed = true)
    private val runtime: TrUAPIHostRuntime = mockk(relaxed = true)
    private val workers: TrUAPIWorkerSupervisor = mockk()

    private fun TestScope.worker(scope: CoroutineScope = CoroutineScope(StandardTestDispatcher(testScheduler))): TrUAPIChatWorker {
        justRun { workers.restartIfFailed(productId) }
        every { workers.executionState(productId) } returns flowOf(WorkerExecutionState.Running(execution))
        every { workers.currentExecution(productId) } returns execution
        return TrUAPIChatWorker(productId, runtime, workers, FakeChatMessaging(), scope)
    }

    // The chat holds the worker for its whole life, and each open room screen holds it once more.
    @Test
    fun `an open room holds the worker until it closes`() = runTest {
        val worker = worker()

        assertTrue(worker.openRoom(room).isSuccess)
        advanceUntilIdle()
        worker.closeRoom(room)

        verify(exactly = 2) { runtime.acquireWorker(productId.value) }
        verify(exactly = 1) { runtime.releaseWorker(productId.value) }
    }

    @Test
    fun `opening a room asks the supervisor to start a worker whose start failed`() = runTest {
        worker().openRoom(room)

        verify { workers.restartIfFailed(productId) }
    }

    @Test
    fun `closing a room that never opened releases nothing`() = runTest {
        worker().closeRoom(room)

        verify(exactly = 0) { runtime.releaseWorker(any()) }
    }

    @Test
    fun `a worker that cannot be held fails the open`() = runTest {
        val worker = worker()
        every { runtime.acquireWorker(productId.value) } throws IllegalStateException("runtime closed")

        assertTrue(worker.openRoom(room).isFailure)
        verify(exactly = 0) { workers.restartIfFailed(any()) }
    }

    @Test
    fun `a disposed chat gives back the rooms that are still open`() = runTest {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val worker = worker(scope)
        worker.openRoom(room)

        scope.cancel()
        advanceUntilIdle()

        verify(exactly = 2) { runtime.releaseWorker(productId.value) }
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
        coEvery { execution.chatMessagesQuery(any(), any()) } throws ChatQueryException.Failed("not connected") andThen
            ProductChatMessagesQueryItem(emptyList(), next = null)

        val page = worker().messagesPage(room, ChatPageAnchorModel.Before(null), pageSize = 40)

        assertTrue(page.isSuccess)
        coVerify(exactly = 2) { execution.chatMessagesQuery(any(), any()) }
    }

    @Test
    fun `a page asked before the client of a restarted worker connects waits for the connection`() = runTest {
        coEvery { execution.chatMessagesQuery(any(), any()) } throws ProductRuntimeException.NotConnected() andThenThrows
            ProductRuntimeException.NotConnected() andThenThrows ProductRuntimeException.NotConnected() andThen
            ProductChatMessagesQueryItem(emptyList(), next = null)

        val page = worker().messagesPage(room, ChatPageAnchorModel.Before(null), pageSize = 40)

        assertTrue(page.isSuccess)
        coVerify(exactly = 4) { execution.chatMessagesQuery(any(), any()) }
    }

    @Test
    fun `a client that never connects fails the page after a limited wait`() = runTest {
        coEvery { execution.chatMessagesQuery(any(), any()) } throws ProductRuntimeException.NotConnected()

        val page = worker().messagesPage(room, ChatPageAnchorModel.Before(null), pageSize = 40)

        assertTrue(page.isFailure)
        assertTrue(currentTime < 20_000)
    }
}
