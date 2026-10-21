package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
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
import uniffi.truapi.ChatWakeOutcome
import uniffi.truapi.ChatWakeReason
import kotlin.time.Duration.Companion.seconds

// The worker holds through wakes of the core (W1 and W2 of 07-LATER §10.2): the core holds a wake for 30 s, and a
// new wake moves its end.
class TrUAPIChatWorkerWakeTest {
    private val productId = ProductId.fromStoredValue("game.dot")
    private val room = ProductChatIdParameter("lobby")
    private val execution: TrUAPIProductExecution = mockk(relaxed = true)
    private val runtime: TrUAPIHostRuntime = mockk(relaxed = true) {
        every { wakeProduct(any(), any()) } returns ChatWakeOutcome.Held(0uL)
    }

    private fun TestScope.worker(
        states: Flow<WorkerExecutionState?> = flowOf(WorkerExecutionState.Running(execution)),
        messaging: FakeChatMessaging = FakeChatMessaging(),
    ): TrUAPIChatWorker {
        val workers: TrUAPIWorkerSupervisor = mockk(relaxed = true)
        every { workers.executionState(productId) } returns states
        every { workers.currentExecution(productId) } returns execution
        return TrUAPIChatWorker(productId, runtime, workers, messaging, CoroutineScope(StandardTestDispatcher(testScheduler)))
    }

    private val messageWake = ChatWakeReason.MessageInFlight("lobby")

    @Test
    fun `a user message wakes the worker before it goes and once more after it goes`() = runTest {
        val worker = worker()

        assertTrue(worker.onUserMessage(room, "hi").isSuccess)
        runCurrent()

        verify(exactly = 2) { runtime.wakeProduct(productId.value, messageWake) }
        verify(exactly = 0) { runtime.acquireWorker(any()) }
    }

    @Test
    fun `a message to a worker that starts slowly wakes it again before the wake ends`() = runTest {
        val states = MutableStateFlow<WorkerExecutionState?>(null)
        val worker = worker(states)

        val sent = backgroundScope.async { worker.onUserMessage(room, "hi") }
        advanceTimeBy(45.seconds)
        verify(exactly = 3) { runtime.wakeProduct(productId.value, messageWake) }

        states.value = WorkerExecutionState.Running(execution)
        runCurrent()
        assertTrue(sent.await().isSuccess)
        advanceTimeBy(60.seconds)

        // The first wake, two renewals at 20 s and 40 s, and the wake after the message: none after that.
        verify(exactly = 4) { runtime.wakeProduct(productId.value, messageWake) }
    }

    @Test
    fun `a user action wakes the worker before it goes and once more after the answer`() = runTest {
        val worker = worker()

        worker.userAction(room, ChatUserActionModel.Retry(messageId = "m1"))
        runCurrent()

        verify(exactly = 2) { runtime.wakeProduct(productId.value, messageWake) }
    }

    @Test
    fun `a press in a message or in the footer wakes the worker after the action goes`() = runTest {
        val worker = worker()

        worker.dispatchFooterEvent(room, "send", JsUiEvent.Type.ButtonClick)
        verify(exactly = 1) { runtime.wakeProduct(productId.value, messageWake) }

        worker.dispatchEvent(JsUiEvent(roomId = room, messageId = "m1", messageType = "card", actionId = "ok", eventType = JsUiEvent.Type.ButtonClick))
        verify(exactly = 2) { runtime.wakeProduct(productId.value, messageWake) }
    }

    @Test
    fun `a wake that the core refuses does not stop the message`() = runTest {
        every { runtime.wakeProduct(any(), any()) } throws IllegalStateException("runtime closed")

        assertTrue(worker().onUserMessage(room, "hi").isSuccess)
    }

    @Test
    fun `a product without a room wakes until its worker runs and 30 s more`() = runTest {
        val states = MutableStateFlow<WorkerExecutionState?>(null)
        val worker = worker(states)

        worker.holdBriefly()
        runCurrent()
        verify(exactly = 1) { runtime.wakeProduct(productId.value, ChatWakeReason.ProductStart) }

        advanceTimeBy(25.seconds)
        verify(exactly = 2) { runtime.wakeProduct(productId.value, ChatWakeReason.ProductStart) }

        states.value = WorkerExecutionState.Running(execution)
        advanceUntilIdle()

        verify(exactly = 3) { runtime.wakeProduct(productId.value, ChatWakeReason.ProductStart) }
        verify(exactly = 0) { runtime.acquireWorker(any()) }
    }

    @Test
    fun `a worker that stops makes the rooms stop to ring`() = runTest {
        val states = MutableStateFlow<WorkerExecutionState?>(null)
        val messaging = FakeChatMessaging()
        worker(states, messaging)
        runCurrent()
        assertEquals("a worker that did not run yet clears nothing", 0, messaging.ringingClears)

        states.value = WorkerExecutionState.Running(execution)
        runCurrent()
        states.value = null
        runCurrent()

        assertEquals(1, messaging.ringingClears)
    }
}
