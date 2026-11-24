package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

private const val ROOM = "jollity"

class ProductRoomFooterCardViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val register = ProductRoomFooter.Card("register", byteArrayOf(1).toDataByteArray())
    private val confirm = ProductRoomFooter.Card("confirm", byteArrayOf(2).toDataByteArray())

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    /** A card the product failed to draw must say so, or the room shows a blank strip and no input. */
    @Test
    fun `a failed render shows the error`() = runTest(testDispatcher) {
        val failure = IllegalStateException("render failed")
        val worker = mockk<ProductWorker> {
            every { renderRoomFooter(any(), any(), any()) } returns flowOf(Result.failure(failure))
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)

        viewModel.show(register)
        advanceUntilIdle()

        assertEquals(LoadingState.Error(failure), viewModel.state.value)
    }

    /** A runtime that cannot draw a docked card must say so, not spin forever. */
    @Test
    fun `a runtime that cannot draw a docked card shows the error`() = runTest(testDispatcher) {
        val viewModel = ProductRoomFooterCardViewModel(ROOM, RecordingWorker())

        viewModel.show(register)
        advanceUntilIdle()

        assertTrue(viewModel.state.value is LoadingState.Error)
    }

    /** A replaced card must stop drawing, or the old render keeps a stream open on the product. */
    @Test
    fun `a replaced card stops the render of the one before`() = runTest(testDispatcher) {
        val registerRender = MutableSharedFlow<Result<JsWidget>>()
        val worker = mockk<ProductWorker> {
            every { renderRoomFooter(any(), "register", any()) } returns registerRender
            every { renderRoomFooter(any(), "confirm", any()) } returns flowOf(Result.success(JsWidget.Spacer()))
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)
        viewModel.show(register)
        advanceUntilIdle()

        viewModel.show(confirm)
        advanceUntilIdle()

        assertEquals(listOf(0, loaded(confirm, JsWidget.Spacer())), listOf(registerRender.subscriptionCount.value, viewModel.state.value))
    }

    /** The screen shows the same card again on every return to the chat, and that must not draw it again. */
    @Test
    fun `showing the same card again keeps its render`() = runTest(testDispatcher) {
        val worker = mockk<ProductWorker>(relaxed = true) {
            every { renderRoomFooter(any(), any(), any()) } returns flowOf(Result.success(JsWidget.Spacer()))
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)

        viewModel.show(register)
        viewModel.show(ProductRoomFooter.Card("register", byteArrayOf(1).toDataByteArray()))
        advanceUntilIdle()
        viewModel.handleUiEvent("join", JsUiEvent.Type.ButtonClick)

        verify(exactly = 1) { worker.renderRoomFooter(ProductChatIdParameter(ROOM), "register", any()) }
        verify { worker.dispatchRoomFooterEvent(ProductChatIdParameter(ROOM), "register", "join", JsUiEvent.Type.ButtonClick) }
    }

    /**
     * The product's next trees for the docked card replace it in place, with no spinner between them: the
     * footer never goes back to loading for the same card.
     */
    @Test
    fun `new trees of the same card replace it in place`() = runTest(testDispatcher) {
        val first = JsWidget.Column(children = listOf(JsWidget.Text("Join")))
        val second = JsWidget.Column(children = listOf(JsWidget.Text("Joined")))
        val trees = MutableSharedFlow<Result<JsWidget>>()
        val worker = mockk<ProductWorker>(relaxed = true) {
            every { renderRoomFooter(any(), any(), any()) } returns trees
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)
        val seen = mutableListOf<LoadingState<ProductRoomFooterCardViewModel.DockedTree>>()
        viewModel.show(register)
        advanceUntilIdle()

        trees.emit(Result.success(first))
        advanceUntilIdle()
        seen += viewModel.state.value
        trees.emit(Result.success(second))
        advanceUntilIdle()
        seen += viewModel.state.value

        assertEquals(listOf(loaded(register, first), loaded(register, second)), seen)
    }

    /**
     * A card the product docks in place of another keeps the old card's tree on screen until its own first
     * tree comes, so the footer can fade from one card to the next instead of flashing a spinner.
     */
    @Test
    fun `a replaced card stays drawn until the new card's first tree`() = runTest(testDispatcher) {
        val registerTree = JsWidget.Text("Join")
        val confirmTree = JsWidget.Text("Joined")
        val confirmRender = MutableSharedFlow<Result<JsWidget>>()
        val worker = mockk<ProductWorker> {
            every { renderRoomFooter(any(), "register", any()) } returns flowOf(Result.success(registerTree))
            every { renderRoomFooter(any(), "confirm", any()) } returns confirmRender
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)
        viewModel.show(register)
        advanceUntilIdle()

        viewModel.show(confirm)
        advanceUntilIdle()
        val beforeItsTree = viewModel.state.value
        confirmRender.emit(Result.success(confirmTree))
        advanceUntilIdle()

        assertEquals(listOf(loaded(register, registerTree), loaded(confirm, confirmTree)), listOf(beforeItsTree, viewModel.state.value))
    }

    /** A press on the tree of a card the product has replaced must not reach the product as a press on the new card. */
    @Test
    fun `a press on a replaced card's tree goes nowhere`() = runTest(testDispatcher) {
        val worker = mockk<ProductWorker>(relaxed = true) {
            every { renderRoomFooter(any(), any(), any()) } returns flowOf(Result.success(JsWidget.Spacer()))
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)
        viewModel.show(register)
        viewModel.show(confirm)
        advanceUntilIdle()

        viewModel.handleUiEvent(register, "join", JsUiEvent.Type.ButtonClick)
        viewModel.handleUiEvent(confirm, "done", JsUiEvent.Type.ButtonClick)

        verify(exactly = 0) { worker.dispatchRoomFooterEvent(any(), any(), "join", any()) }
        verify(exactly = 1) { worker.dispatchRoomFooterEvent(ProductChatIdParameter(ROOM), "confirm", "done", JsUiEvent.Type.ButtonClick) }
    }

    private fun loaded(card: ProductRoomFooter.Card, tree: JsWidget) =
        LoadingState.Loaded(ProductRoomFooterCardViewModel.DockedTree(card, tree))
}
