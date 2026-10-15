package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.RoomParticipation
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.argThat
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import uniffi.truapi.ChatActionPayload
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.HostChatActionSubscribeItem
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.ProductRuntimeException
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes

class TrUAPIChatWorkerTest {
    private val productId = ProductId.fromStoredValue("chat.dot")
    private val roomId = ProductChatIdParameter("room-1")
    private val messageId: ChatMessageId = "msg-1"
    private val messageType = "custom.widget"
    private val messageData = DataByteArray.empty()

    private fun TestScope.worker(
        runtime: TrUAPIHostRuntime = mock(),
        workers: TrUAPIWorkerSupervisor = mock(),
        chatMessaging: ProductChatMessaging = FakeChatMessaging(),
        scope: CoroutineScope = CoroutineScope(StandardTestDispatcher(testScheduler)),
    ): TrUAPIChatWorker = TrUAPIChatWorker(productId, runtime, workers, chatMessaging, scope)

    private fun runningWorkers(execution: TrUAPIProductExecution): TrUAPIWorkerSupervisor {
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(flowOf(WorkerExecutionState.Running(execution)))
        whenever(workers.currentExecution(productId)).thenReturn(execution)
        return workers
    }

    @Test
    fun `a stream that draws renders once, against the room and message it was given`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        whenever(execution.render(any())).thenReturn(flowOf(RendererNode.Nil))
        val worker = worker(workers = runningWorkers(execution))

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(listOf(Result.success(JsWidget.Spacer())), results)
        var captured: ProductRendererRenderRequest? = null
        verify(execution, times(1)).render(argThat { captured = it; true })
        assertEquals(
            RenderContext.ChatMessage(roomId = roomId.value, messageId = messageId, messageType = messageType),
            requireNotNull(captured).context,
        )
    }

    @Test
    fun `a Failed execution state whose cause is a TimeoutCancellationException still yields a failure value`() = runTest {
        val timeoutCause = try {
            withTimeout(1.milliseconds) { awaitCancellation() }
            error("unreachable: withTimeout should have thrown")
        } catch (e: TimeoutCancellationException) {
            e
        }
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(flowOf(WorkerExecutionState.Failed(timeoutCause)))
        val worker = worker(workers = workers)

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(1, results.size)
        val failure = results.single().exceptionOrNull()
        assertFalse("the value that reaches the collector must not itself be a cancellation", failure is CancellationException)
        assertSame(timeoutCause, failure?.cause)
    }

    @Test
    fun `a stream that ends without drawing is reopened`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        whenever(execution.render(any())).thenReturn(emptyFlow(), flowOf(RendererNode.Nil))
        val worker = worker(workers = runningWorkers(execution))

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(listOf(Result.success(JsWidget.Spacer())), results)
        verify(execution, times(2)).render(any())
    }

    @Test
    fun `a stream that fails outright is reopened`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        whenever(execution.render(any())).thenReturn(flow { throw IllegalStateException("boom") }, flowOf(RendererNode.Nil))
        val worker = worker(workers = runningWorkers(execution))

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(listOf(Result.success(JsWidget.Spacer())), results)
        verify(execution, times(2)).render(any())
    }

    @Test
    fun `a stream that fails after drawing keeps the drawn node and reopens without emitting a failure`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val firstAttempt = flow<RendererNode> {
            emit(RendererNode.Nil)
            throw IllegalStateException("boom")
        }
        whenever(execution.render(any())).thenReturn(firstAttempt, flowOf(RendererNode.Nil))
        val worker = worker(workers = runningWorkers(execution))

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(listOf(Result.success(JsWidget.Spacer()), Result.success(JsWidget.Spacer())), results)
        verify(execution, times(2)).render(any())
    }

    @Test
    fun `a stream that keeps failing is reopened for as long as the cell is collected`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        var opened = 0
        whenever(execution.render(any())).thenReturn(
            flow {
                opened++
                emit(RendererNode.Nil)
                throw IllegalStateException("boom")
            },
        )
        val worker = worker(workers = runningWorkers(execution), scope = CoroutineScope(StandardTestDispatcher(testScheduler)))

        val results = mutableListOf<Result<JsWidget>>()
        val collector = launch {
            worker.renderMessage(roomId, messageId, messageType, messageData).collect { results += it }
        }
        advanceTimeBy(5.minutes)

        // Gaps of 1, 2, 4, 8, 16 and then 30 seconds: five minutes hold 14 openings, past any bounded attempt count.
        assertEquals(14, opened)
        assertTrue("a node that drew must not be replaced by an error", results.all { it.isSuccess })
        collector.cancel()
    }

    @Test
    fun `a NotConnected refusal is retried on the same render`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        var collected = 0
        val flakyThenDraws = flow<RendererNode> {
            collected++
            if (collected == 1) throw ProductRuntimeException.NotConnected()
            emit(RendererNode.Nil)
        }
        whenever(execution.render(any())).thenReturn(flakyThenDraws)
        val worker = worker(workers = runningWorkers(execution))

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(listOf(Result.success(JsWidget.Spacer())), results)
        assertEquals(2, collected)
        verify(execution, times(1)).render(any())
    }

    @Test
    fun `a failed execution reports the failure without ending the cell`() = runTest {
        val replacement: TrUAPIProductExecution = mock()
        whenever(replacement.render(any())).thenReturn(flowOf(RendererNode.Nil))
        val states = MutableStateFlow<WorkerExecutionState?>(
            WorkerExecutionState.Failed(IllegalStateException("boot failed")),
        )
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(states)
        val worker = worker(workers = workers, scope = CoroutineScope(StandardTestDispatcher(testScheduler)))

        val results = mutableListOf<Result<JsWidget>>()
        val collector = launch {
            worker.renderMessage(roomId, messageId, messageType, messageData).collect { results += it }
        }
        advanceUntilIdle()
        assertTrue(results.single().isFailure)

        states.value = WorkerExecutionState.Running(replacement)
        advanceUntilIdle()

        assertEquals(Result.success(JsWidget.Spacer()), results.last())
        collector.cancel()
    }

    @Test
    fun `a disposed worker stops retrying instead of reopening`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        var renderCancelled = false
        whenever(execution.render(any())).thenReturn(
            flow {
                try {
                    awaitCancellation()
                } finally {
                    renderCancelled = true
                }
            },
        )
        val states = MutableStateFlow<WorkerExecutionState?>(WorkerExecutionState.Running(execution))
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(states)
        val worker = worker(workers = workers, scope = CoroutineScope(StandardTestDispatcher(testScheduler)))

        val results = mutableListOf<Result<JsWidget>>()
        val collector = launch {
            worker.renderMessage(roomId, messageId, messageType, messageData).collect { results += it }
        }
        advanceUntilIdle()
        verify(execution, times(1)).render(any())

        states.value = null
        advanceUntilIdle()

        assertTrue("the render must be cancelled when its execution goes away", renderCancelled)
        assertTrue(results.isEmpty())
        verify(execution, times(1)).render(any())

        collector.cancel()
    }

    @Test
    fun `a render on a worker whose scope is already cancelled still draws`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        whenever(execution.render(any())).thenReturn(flowOf(RendererNode.Nil))
        val workerScope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val worker = worker(workers = runningWorkers(execution), scope = workerScope)

        workerScope.cancel()
        advanceUntilIdle()

        val results = worker.renderMessage(roomId, messageId, messageType, messageData).toList()

        assertEquals(listOf(Result.success(JsWidget.Spacer())), results)
        verify(execution, times(1)).render(any())
    }

    @Test
    fun `a render whose execution dies before drawing is redrawn on the replacement execution`() = runTest {
        val dying: TrUAPIProductExecution = mock()
        val replacement: TrUAPIProductExecution = mock()
        whenever(dying.render(any())).thenReturn(flow { throw CancellationException("execution disposed") })
        whenever(replacement.render(any())).thenReturn(flowOf(RendererNode.Nil))
        val states = MutableStateFlow<WorkerExecutionState?>(WorkerExecutionState.Running(dying))
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(states)
        val worker = worker(workers = workers, scope = CoroutineScope(StandardTestDispatcher(testScheduler)))

        val results = mutableListOf<Result<JsWidget>>()
        val collector = launch {
            worker.renderMessage(roomId, messageId, messageType, messageData).collect { results += it }
        }
        advanceUntilIdle()

        assertTrue("the dying execution must not end the cell's flow", collector.isActive)

        states.value = WorkerExecutionState.Running(replacement)
        advanceUntilIdle()

        // The whole list, not lastOrNull(): a failure emitted here is a visible flash in the cell.
        assertEquals(listOf(Result.success(JsWidget.Spacer())), results)
        verify(replacement, times(1)).render(any())
        verify(dying, times(1)).render(any())

        collector.cancel()
    }

    @Test
    fun `an exception from the collector propagates instead of reopening the render`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        whenever(execution.render(any())).thenReturn(flowOf(RendererNode.Nil))
        val worker = worker(workers = runningWorkers(execution))
        val boom = IllegalStateException("the feed blew up")

        val thrown = runCatching {
            worker.renderMessage(roomId, messageId, messageType, messageData).collect { throw boom }
        }.exceptionOrNull()

        // kotlinx copies the exception for stack-trace recovery, so identity is not preserved.
        assertEquals(boom.message, thrown?.message)
        verify(execution, times(1)).render(any())
    }

    @Test
    fun `cancelling the collector stops the render instead of leaking it`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val replacement: TrUAPIProductExecution = mock()
        var rendering = false
        whenever(execution.render(any())).thenReturn(
            flow {
                rendering = true
                try {
                    awaitCancellation()
                } finally {
                    rendering = false
                }
            },
        )
        val states = MutableStateFlow<WorkerExecutionState?>(WorkerExecutionState.Running(execution))
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(states)
        val worker = worker(workers = workers, scope = CoroutineScope(StandardTestDispatcher(testScheduler)))

        val collector = launch {
            worker.renderMessage(roomId, messageId, messageType, messageData).collect { }
        }
        advanceUntilIdle()
        assertTrue(rendering)

        collector.cancel()
        advanceUntilIdle()

        assertFalse("the render must not outlive its collector", rendering)
        states.value = WorkerExecutionState.Running(replacement)
        advanceUntilIdle()
        verify(replacement, never()).render(any())
    }

    @Test
    fun `dispatchEvent publishes a renderer action addressed by RenderContext ChatMessage`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.currentExecution(productId)).thenReturn(execution)
        whenever(workers.executionState(productId)).thenReturn(emptyFlow())
        val worker = worker(workers = workers)

        worker.dispatchEvent(
            JsUiEvent(messageId, messageType, actionId = "onTap", eventType = JsUiEvent.Type.ButtonClick, roomId = roomId),
        )
        worker.dispatchEvent(
            JsUiEvent(
                messageId,
                messageType,
                actionId = "onChange",
                eventType = JsUiEvent.Type.InputFieldValueChange("hi"),
                roomId = roomId,
            ),
        )

        val captured = mutableListOf<HostRendererActionSubscribeItem>()
        verify(execution, times(2)).publishRendererAction(argThat { captured += it; true })
        assertEquals(RenderContext.ChatMessage(roomId.value, messageId, messageType), captured[0].context)
        assertEquals("onTap", captured[0].actionId)
        assertTrue(captured[0].payload.isEmpty())
        assertEquals("hi", captured[1].payload.toString(Charsets.UTF_8))
    }

    @Test
    fun `onUserMessage publishes a chat action with peer native`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val worker = worker(workers = runningWorkers(execution))

        val result = worker.onUserMessage(roomId, "hello")

        assertTrue(result.isSuccess)
        var captured: HostChatActionSubscribeItem? = null
        verify(execution).publishChatAction(argThat { captured = it; true })
        assertEquals(
            HostChatActionSubscribeItem(roomId.value, "native", ChatActionPayload.MessagePosted(ChatMessageContent.Text("hello"))),
            captured,
        )
    }

    @Test
    fun `onUserMessage on a failed execution fails with the boot cause`() = runTest {
        val cause = IllegalStateException("worker boot failed")
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(flowOf(WorkerExecutionState.Failed(cause)))
        val worker = worker(workers = workers)

        val result = worker.onUserMessage(roomId, "hello")

        assertSame(cause, result.exceptionOrNull()?.cause)
    }

    @Test
    fun `onUserMessage gives up when no execution is ever reported`() = runTest {
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(MutableStateFlow(null))
        val worker = worker(workers = workers)

        val result = worker.onUserMessage(roomId, "hello")

        assertTrue(result.exceptionOrNull() is IllegalStateException)
    }

    @Test
    fun `a publishChatAction failure is returned, not thrown`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val boom = ProductRuntimeException.Denied()
        whenever(execution.publishChatAction(any())).thenThrow(boom)
        val worker = worker(workers = runningWorkers(execution))

        val result = worker.onUserMessage(roomId, "hello")

        assertSame(boom, result.exceptionOrNull())
    }

    @Test
    fun `a message with no room id is refused instead of reaching the core`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val worker = worker(workers = runningWorkers(execution))

        val result = worker.onUserMessage(null, "hello")

        assertTrue(result.exceptionOrNull() is IllegalArgumentException)
        verify(execution, never()).publishChatAction(any())
    }

    @Test
    fun `a render with no room id fails instead of waiting on the core`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val worker = worker(workers = runningWorkers(execution))

        val result = worker.renderMessage(null, messageId, messageType, messageData).first()

        assertTrue(result.exceptionOrNull() is IllegalArgumentException)
        verify(execution, never()).render(any())
    }

    @Test
    fun `an event with no room id is dropped instead of reaching the core`() = runTest {
        val execution: TrUAPIProductExecution = mock()
        val worker = worker(workers = runningWorkers(execution))

        worker.dispatchEvent(JsUiEvent(messageId, messageType, "action", JsUiEvent.Type.ButtonClick, roomId = null))

        verify(execution, never()).publishRendererAction(any())
    }

    @Test
    fun `onUserMessage propagates a genuine cancellation instead of returning a Result`() = runTest {
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(flow { awaitCancellation() })
        val worker = worker(workers = workers)

        var result: Result<Unit>? = null
        val caller = launch {
            result = worker.onUserMessage(roomId, "hello")
        }
        runCurrent()

        caller.cancel()
        advanceUntilIdle()

        assertTrue(caller.isCancelled)
        assertNull("a swallowed cancellation would have let onUserMessage return a Result", result)
    }

    @Test
    fun `forwarding starts on the execution that follows a failed boot, and moves to a replacement`() = runTest {
        val first: TrUAPIProductExecution = mock()
        val replacement: TrUAPIProductExecution = mock()
        val rooms = MutableSharedFlow<List<ProductChatRoom>>()
        val chatMessaging = FakeChatMessaging(rooms = rooms)
        val states = MutableStateFlow<WorkerExecutionState?>(WorkerExecutionState.Failed(IllegalStateException("boot failed")))
        val workers: TrUAPIWorkerSupervisor = mock()
        whenever(workers.executionState(productId)).thenReturn(states)

        worker(workers = workers, chatMessaging = chatMessaging, scope = CoroutineScope(StandardTestDispatcher(testScheduler)))
        advanceUntilIdle()

        states.value = WorkerExecutionState.Running(first)
        advanceUntilIdle()
        rooms.emit(listOf(ProductChatRoom(roomId.value, RoomParticipation.ROOM_HOST)))
        advanceUntilIdle()
        verify(first, times(1)).notifyChatRoomsChanged(any())

        states.value = WorkerExecutionState.Running(replacement)
        advanceUntilIdle()
        rooms.emit(listOf(ProductChatRoom(roomId.value, RoomParticipation.ROOM_HOST)))
        advanceUntilIdle()

        verify(replacement, times(1)).notifyChatRoomsChanged(any())
        verify(first, times(1)).notifyChatRoomsChanged(any())
    }

    @Test
    fun `the lease is acquired once and released once when the scope is cancelled`() = runTest {
        val runtime: TrUAPIHostRuntime = mock()
        val workerScope = CoroutineScope(StandardTestDispatcher(testScheduler))

        worker(runtime = runtime, scope = workerScope)

        verify(runtime, times(1)).acquireWorker(productId.value)
        verify(runtime, never()).releaseWorker(productId.value)

        workerScope.cancel()
        advanceUntilIdle()

        verify(runtime, times(1)).releaseWorker(productId.value)
    }
}
