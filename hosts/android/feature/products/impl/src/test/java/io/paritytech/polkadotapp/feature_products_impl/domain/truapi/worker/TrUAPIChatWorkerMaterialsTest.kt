package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.mockk.verifyOrder
import io.parity.truapi.MaterialsResolution
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialsProps
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ProductId
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode
import uniffi.truapi.ResolvedMaterials

// Chat draws Materials nodes plain, and the RFC asks that the product hear they are unsupported:
// the core reports a chat body's nodes when a resolution of it first answers.
class TrUAPIChatWorkerMaterialsTest {
    private val productId = ProductId.parse("chat.dot").getOrThrow()
    private val roomId = ProductChatIdParameter("room-1")
    private val messageContext = RenderContext.ChatMessage(roomId = roomId.value, messageId = "msg-1", messageType = "custom")
    private val withMaterials = RendererNode.Materials(MaterialsProps(emptyList(), 0uL), listOf(RendererNode.Spacer(emptyList())))

    private val resolution = mockk<MaterialsResolution>(relaxed = true) {
        every { resolved } returns MutableStateFlow(
            ResolvedMaterials(listOf(NodeMaterials.Plain(MaterialFailure.UNSUPPORTED)), noiseSeed = 0u),
        )
    }

    private fun executionDrawing(tree: RendererNode) = mockk<TrUAPIProductExecution>(relaxed = true) {
        every { render(any()) } returns flowOf(tree)
        every { resolveMaterials(any(), any(), any()) } returns resolution
    }

    private fun TestScope.worker(execution: TrUAPIProductExecution) = TrUAPIChatWorker(
        productId = productId,
        runtime = mockk(relaxed = true),
        workers = mockk {
            every { executionState(productId) } returns flowOf(WorkerExecutionState.Running(execution))
            every { currentExecution(productId) } returns execution
        },
        chatMessaging = FakeChatMessaging(),
        scope = CoroutineScope(StandardTestDispatcher(testScheduler)),
    )

    @Test
    fun `a message with a materials node is reported unsupported once and drawn unchanged`() = runTest {
        val execution = executionDrawing(withMaterials)

        val drawn = worker(execution).renderMessage(roomId, "msg-1", "custom", DataByteArray.empty()).toList()
        advanceUntilIdle()

        assertEquals(listOf(Result.success(JsWidget.Materials(0, listOf(JsWidget.Spacer())))), drawn)
        verifyOrder {
            execution.resolveMaterials(withMaterials, messageContext, false)
            resolution.close()
        }
        verify(exactly = 1) { execution.resolveMaterials(any(), any(), any()) }
    }
}
