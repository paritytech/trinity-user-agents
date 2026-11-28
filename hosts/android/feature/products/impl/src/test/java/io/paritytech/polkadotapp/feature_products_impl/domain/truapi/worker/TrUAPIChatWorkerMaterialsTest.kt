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
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import uniffi.truapi.ColumnProps
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialsProps
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ProductId
import uniffi.truapi.RenderContext
import uniffi.truapi.RendererNode
import uniffi.truapi.ResolvedMaterials
import uniffi.truapi.TextProps

// A chat message publishes the materials of the tree it shows for its card to draw. The footer card draws them plain,
// and the RFC asks that the product hear they are unsupported: the core reports them when a resolution first answers.
class TrUAPIChatWorkerMaterialsTest {
    private val productId = ProductId.parse("chat.dot").getOrThrow()
    private val roomId = ProductChatIdParameter("room-1")
    private val messageContext = RenderContext.ChatMessage(roomId = roomId.value, messageId = "msg-1", messageType = "custom")
    private val withMaterials = RendererNode.Materials(MaterialsProps(emptyList()), listOf(RendererNode.Spacer(emptyList())))
    private val withoutMaterials = RendererNode.Column(emptyList(), ColumnProps(horizontalAlignment = null, verticalArrangement = null), emptyList())

    private fun resolution() = mockk<MaterialsResolution>(relaxed = true) {
        every { resolved } returns MutableStateFlow(
            ResolvedMaterials(listOf(NodeMaterials.Plain(MaterialFailure.UNSUPPORTED)), noiseSeed = 0u),
        )
    }

    private fun executionDrawing(trees: Flow<RendererNode>, vararg resolutions: MaterialsResolution) =
        mockk<TrUAPIProductExecution>(relaxed = true) {
            every { render(any()) } returns trees
            if (resolutions.isNotEmpty()) every { resolveMaterials(any(), any(), any()) } returnsMany resolutions.toList()
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

    private fun TrUAPIChatWorker.renderCard() = renderMessage(roomId, "msg-1", "custom", DataByteArray.empty())

    private fun TrUAPIChatWorker.cardMaterials() = messageMaterials(roomId, "msg-1", "custom")

    @Test
    fun `a new tree opens its materials before the last one closes, and the end of the card closes them`() = runTest {
        val first = resolution()
        val second = resolution()
        val trees = MutableSharedFlow<RendererNode>()
        val secondTree = RendererNode.Materials(MaterialsProps(emptyList()), emptyList())
        val execution = executionDrawing(trees, first, second)
        val worker = worker(execution)
        val published = mutableListOf<MaterialsResolution?>()
        val materials = launch { worker.cardMaterials().toList(published) }
        val render = launch { worker.renderCard().collect { } }
        runCurrent()

        trees.emit(withMaterials)
        runCurrent()
        trees.emit(secondTree)
        runCurrent()
        render.cancel()
        runCurrent()
        materials.cancel()

        assertEquals(listOf(null, first, second, null), published)
        verifyOrder {
            execution.resolveMaterials(withMaterials, messageContext, false)
            execution.resolveMaterials(secondTree, messageContext, false)
            first.close()
            second.close()
        }
    }

    @Test
    fun `a tree without materials resolves none`() = runTest {
        val execution = executionDrawing(flowOf(withoutMaterials))

        worker(execution).renderCard().toList()

        verify(exactly = 0) { execution.resolveMaterials(any(), any(), any()) }
    }

    @Test
    fun `a materials node under a text still resolves the tree`() = runTest {
        val tree = RendererNode.Text(emptyList(), TextProps(style = null, color = null), listOf(withMaterials))
        val execution = executionDrawing(flowOf(tree), resolution())

        worker(execution).renderCard().toList()

        verify(exactly = 1) { execution.resolveMaterials(tree, messageContext, false) }
    }

    @Test
    fun `a tree that drops its materials unpublishes them while the card stays open`() = runTest {
        val execution = executionDrawing(
            flow {
                emit(withMaterials)
                emit(withoutMaterials)
                awaitCancellation()
            },
            resolution(),
        )
        val worker = worker(execution)
        val render = launch { worker.renderCard().collect { } }
        runCurrent()

        val published = worker.cardMaterials().first()
        render.cancel()

        assertNull(published)
    }

    @Test
    fun `a collector that ends leaves the materials another collector of the message published`() = runTest {
        val first = resolution()
        val second = resolution()
        val execution = executionDrawing(
            flow {
                emit(withMaterials)
                awaitCancellation()
            },
            first,
            second,
        )
        val worker = worker(execution)
        val earlier = launch { worker.renderCard().collect { } }
        runCurrent()
        val later = launch { worker.renderCard().collect { } }
        runCurrent()

        earlier.cancel()
        runCurrent()
        val published = worker.cardMaterials().first()
        later.cancel()

        assertSame(second, published)
    }

    @Test
    fun `a footer card with a materials node is reported unsupported once and drawn unchanged`() = runTest {
        val resolution = resolution()
        val execution = executionDrawing(flowOf(withMaterials), resolution)
        val footerContext = RenderContext.ChatRoomFooter(roomId = roomId.value, messageType = "register")

        val drawn = worker(execution).renderRoomFooter(roomId, "register", DataByteArray.empty()).toList()
        advanceUntilIdle()

        assertEquals(listOf(Result.success(JsWidget.Materials(0, listOf(JsWidget.Spacer())))), drawn)
        verifyOrder {
            execution.resolveMaterials(withMaterials, footerContext, false)
            resolution.close()
        }
        verify(exactly = 1) { execution.resolveMaterials(any(), any(), any()) }
    }
}
