package io.paritytech.polkadotapp.feature_products_impl.domain.bot.message

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

private const val ROOM = "jollity"
private const val MESSAGE_TYPE = "weekly-draw"

class ProductMessageRendersTest {
    private val product = Product(ProductId.fromStoredValue("dim2.dot"), "Jollity", icon = null)
    private val room = ProductChatIdParameter(ROOM)
    private val content = ProductsMessageContent(MESSAGE_TYPE, ByteArray(0).toDataByteArray())
    private val window = 5.seconds
    private val keepAlive = 30.seconds

    /** Every render is served from this one stream. */
    private val trees = MutableSharedFlow<Result<JsWidget>>()
    private var opened = 0
    private val streamingWorker: ProductWorker = mockk {
        every { renderMessage(any(), any(), any(), any()) } answers { opened++; trees }
    }

    private fun TestScope.renders(worker: ProductWorker, observers: Set<ProductMessageRenderObserver> = emptySet()) =
        ProductMessageRenders(
            product,
            worker,
            StandardTestDispatcher(testScheduler),
            observers,
            firstTreeWindow = window,
            streamKeepAlive = keepAlive,
        )

    /** A cell on screen: observes the state until cancelled. */
    private fun TestScope.shown(state: StateFlow<LoadingState<JsWidget>>): Job = launch { state.collect {} }

    private suspend fun drawTree() {
        trees.emit(Result.success(JsWidget.Spacer()))
    }

    @Test
    fun `renders against the message's own room, not one derived from the product`() = runTest {
        val worker = RecordingWorker()

        val cell = shown(renders(worker).render(room, "m1", content))
        runCurrent()

        assertEquals(listOf(room), worker.renderedRooms)
        cell.cancel()
    }

    @Test
    fun `a ui event carries the room and message type the core routes by`() = runTest {
        val worker = RecordingWorker()

        renders(worker).dispatchEvent(room, "m1", content, "press", JsUiEvent.Type.ButtonClick)

        val event = worker.events.single()
        assertEquals(listOf("m1", MESSAGE_TYPE, "press", room), listOf(event.messageId, event.messageType, event.actionId, event.roomId))
    }

    @Test
    fun `a message nobody shows opens no stream`() = runTest {
        renders(streamingWorker).render(room, "m1", content)
        advanceTimeBy(window + 1.milliseconds)

        assertEquals(0, opened)
    }

    @Test
    fun `a message shown again reads its last tree without a second render`() = runTest {
        val renders = renders(streamingWorker)
        val first = renders.render(room, "m1", content)
        val cell = shown(first)
        runCurrent()
        drawTree()
        runCurrent()
        cell.cancel()

        val again = renders.render(room, "m1", content)
        val cellAgain = shown(again)
        runCurrent()

        assertEquals(LoadingState.Loaded(JsWidget.Spacer()), again.value)
        assertSame(first.value, again.value)
        assertEquals(1, opened)
        cellAgain.cancel()
    }

    @Test
    fun `a cell left for longer than the keep-alive closes its stream, and reopens it behind its tree when shown again`() = runTest {
        val state = renders(streamingWorker).render(room, "m1", content)
        val cell = shown(state)
        runCurrent()
        drawTree()
        runCurrent()
        cell.cancel()
        advanceTimeBy(keepAlive + 1.milliseconds)

        assertEquals("the stream must not outlive the cell", 0, trees.subscriptionCount.value)
        assertEquals(LoadingState.Loaded(JsWidget.Spacer()), state.value)

        val cellAgain = shown(state)
        runCurrent()
        advanceTimeBy(window + 1.milliseconds)

        assertEquals(2, opened)
        assertEquals("a reopened stream that has drawn nothing yet keeps the tree", LoadingState.Loaded(JsWidget.Spacer()), state.value)
        cellAgain.cancel()
    }

    @Test
    fun `a message with no tree inside the window reads as failed, then draws when its tree arrives`() = runTest {
        val state = renders(streamingWorker).render(room, "m1", content)
        val cell = shown(state)
        advanceTimeBy(window - 1.milliseconds)
        assertEquals(LoadingState.Loading, state.value)
        advanceTimeBy(2.milliseconds)
        assertTrue(state.value is LoadingState.Error)

        drawTree()
        runCurrent()

        assertEquals(LoadingState.Loaded(JsWidget.Spacer()), state.value)
        cell.cancel()
    }

    @Test
    fun `a tree drawn inside the window is not replaced when the window passes`() = runTest {
        val state = renders(streamingWorker).render(room, "m1", content)
        val cell = shown(state)
        runCurrent()
        drawTree()
        runCurrent()

        advanceTimeBy(window + 1.milliseconds)

        assertEquals(LoadingState.Loaded(JsWidget.Spacer()), state.value)
        cell.cancel()
    }

    @Test
    fun `a failed message is rendered again the next time it is shown`() = runTest {
        val renders = renders(streamingWorker)
        val cell = shown(renders.render(room, "m1", content))
        advanceTimeBy(window + 1.milliseconds)
        cell.cancel()

        val again = renders.render(room, "m1", content)
        val cellAgain = shown(again)
        runCurrent()

        assertEquals(LoadingState.Loading, again.value)
        assertEquals(2, opened)
        cellAgain.cancel()
    }

    @Test
    fun `a worker that throws reads as failed instead of taking the process down`() = runTest {
        val worker: ProductWorker = mockk {
            every { renderMessage(any(), any(), any(), any()) } throws IllegalStateException("no runtime")
        }

        val state = renders(worker).render(room, "m1", content)
        val cell = shown(state)
        runCurrent()

        assertEquals("no runtime", (state.value as LoadingState.Error).exception.message)
        cell.cancel()
    }

    @Test
    fun `closing fails the cells still loading and keeps the trees drawn`() = runTest {
        val renders = renders(streamingWorker)
        val drawn = renders.render(room, "m1", content)
        val drawnCell = shown(drawn)
        runCurrent()
        drawTree()
        runCurrent()
        val loading = renders.render(room, "m2", content)
        val loadingCell = shown(loading)
        runCurrent()

        renders.close()

        assertEquals(LoadingState.Loaded(JsWidget.Spacer()), drawn.value)
        assertTrue(loading.value is LoadingState.Error)
        assertTrue(renders.render(room, "m3", content).value is LoadingState.Error)
        drawnCell.cancel()
        loadingCell.cancel()
    }

    @Test
    fun `each tree reaching the screen is reported to the observers`() = runTest {
        val rendered = mutableListOf<ProductId>()
        val observer = object : ProductMessageRenderObserver {
            override fun onMessageRendered(productId: ProductId) {
                rendered += productId
            }
        }

        val cell = shown(renders(streamingWorker, setOf(observer)).render(room, "m1", content))
        runCurrent()
        drawTree()
        drawTree()
        runCurrent()

        assertEquals(listOf(product.id, product.id), rendered)
        cell.cancel()
    }
}
