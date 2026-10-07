package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat

import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.HostRendererActionSubscribeItem
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode

class TrUAPIChatProductWorkerTest {
    private val requests = mutableListOf<ProductRendererRenderRequest>()
    private val action = slot<HostRendererActionSubscribeItem>()

    private val execution = mockk<TrUAPIProductExecution> {
        every { render(any()) } answers {
            requests += firstArg<ProductRendererRenderRequest>()
            flowOf(RendererNode.Nil)
        }
        every { publishRendererAction(capture(action)) } returns Unit
    }

    private val workers = mockk<TrUAPIWorkerSupervisor> {
        every { execution(PRODUCT) } returns flowOf(execution)
        every { currentExecution(PRODUCT) } returns execution
    }

    private val worker = TrUAPIChatProductWorker(PRODUCT, EXTENSION, workers)

    // The guest only draws a body whose context names its room, message and type; the room is the
    // product's own id for the chat, not the extension-level chat the legacy path used.
    @Test
    fun `a custom message renders its own chat context on the worker`() = runTest {
        val widget = worker.renderMessage(ROOM_CHAT, "message-1", "counter/v1", DataByteArray(byteArrayOf(7))).first()

        assertTrue(widget.isSuccess)
        assertEquals(CONTEXT, requests.single().context)
        assertArrayEquals(byteArrayOf(7), requests.single().payload)
    }

    @Test
    fun `a press in a rendered body goes back as a renderer action on its context`() = runTest {
        worker.renderMessage(ROOM_CHAT, "message-1", "counter/v1", DataByteArray.empty()).first()

        worker.dispatchEvent(JsUiEvent("message-1", ROOM_CHAT, "bump", JsUiEvent.Type.ButtonClick))

        assertEquals(CONTEXT, action.captured.context)
        assertEquals("bump", action.captured.actionId)
        assertArrayEquals(ByteArray(0), action.captured.payload)
    }

    // Without the context it was drawn in, the core could not route the press to the right body.
    @Test
    fun `a press on a body never rendered here is dropped`() {
        worker.dispatchEvent(JsUiEvent("message-9", ROOM_CHAT, "bump", JsUiEvent.Type.ButtonClick))

        verify(exactly = 0) { execution.publishRendererAction(any()) }
    }

    @Test
    fun `a message outside the product's rooms fails instead of rendering`() = runTest {
        val foreign = ChatId.forExtensionRoom("ProductBot_other.paseo", "counter")

        val widget = worker.renderMessage(foreign, "message-1", "counter/v1", DataByteArray.empty()).first()

        assertTrue(widget.isFailure)
        assertTrue(requests.isEmpty())
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("counter.paseo")
        val EXTENSION = PRODUCT.toChatExtensionId()
        val ROOM_CHAT = ChatId.forExtensionRoom(EXTENSION, "counter")
        val CONTEXT = RenderContext.ChatMessage(roomId = "counter", messageId = "message-1", messageType = "counter/v1")
    }
}
