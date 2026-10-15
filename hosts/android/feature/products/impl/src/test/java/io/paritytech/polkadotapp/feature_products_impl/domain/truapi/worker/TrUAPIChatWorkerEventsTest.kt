package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductRoomEvent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ActionTrigger
import uniffi.truapi.ChatCommand
import uniffi.truapi.ChatEvent
import uniffi.truapi.ChatMessageBody

class TrUAPIChatWorkerEventsTest {
    private val productId = ProductId.fromStoredValue("game.dot")
    private val room = ProductChatIdParameter("lobby")
    private val execution: TrUAPIProductExecution = mockk(relaxed = true)

    private fun TestScope.worker(): TrUAPIChatWorker {
        val workers: TrUAPIWorkerSupervisor = mockk()
        justRun { workers.restartIfFailed(productId) }
        every { workers.executionState(productId) } returns flowOf(WorkerExecutionState.Running(execution))
        val runtime: TrUAPIHostRuntime = mockk(relaxed = true)
        return TrUAPIChatWorker(productId, runtime, workers, FakeChatMessaging(), CoroutineScope(StandardTestDispatcher(testScheduler)))
    }

    @Test
    fun `each room event is published as its chat extension v2 event with peer native`() = runTest {
        val worker = worker()

        worker.publishRoomEvent(room, ProductRoomEvent.MessagePosted("h1", ChatMessage.Content.Text("hi"), replyTo = "m0"))
        worker.publishRoomEvent(room, ProductRoomEvent.UserEdited("h1", ChatMessage.Content.RichText("hey", emptyList())))
        worker.publishRoomEvent(room, ProductRoomEvent.UserDeleted("h1"))
        worker.publishRoomEvent(room, ProductRoomEvent.UserReacted("m0", "👍", placed = false))
        worker.publishRoomEvent(room, ProductRoomEvent.Seen("m0"))
        worker.publishRoomEvent(room, ProductRoomEvent.ActionTriggered("m0", "yes"))
        worker.publishRoomEvent(room, ProductRoomEvent.Command("roll", "2"))
        worker.publishRoomEvent(room, ProductRoomEvent.RoomOpened)

        verify { execution.publishChatEvent("lobby", "native", ChatEvent.MessagePosted("h1", ChatMessageBody.Text("hi"), "m0")) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.UserEdited("h1", ChatMessageBody.Text("hey"))) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.UserDeleted("h1")) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.UserReacted("m0", "👍", false)) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.Seen("m0")) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.ActionTriggered(ActionTrigger("m0", "yes", null))) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.Command(ChatCommand("roll", "2"))) }
        verify { execution.publishChatEvent("lobby", "native", ChatEvent.RoomOpened) }
    }

    @Test
    fun `a publish failure is returned, not thrown`() = runTest {
        every { execution.publishChatEvent(any(), any(), any()) } throws IllegalStateException("closed")

        val result = worker().publishRoomEvent(room, ProductRoomEvent.Seen("m0"))

        assertTrue(result.isFailure)
    }
}
