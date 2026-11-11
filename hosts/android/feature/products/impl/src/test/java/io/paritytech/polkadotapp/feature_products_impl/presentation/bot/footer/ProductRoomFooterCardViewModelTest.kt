package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductRoomFooter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
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
import org.junit.Before
import org.junit.Test

private const val ROOM = "jollity"

class ProductRoomFooterCardViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val register = ProductRoomFooter.Card("register", byteArrayOf(1))
    private val confirm = ProductRoomFooter.Card("confirm", byteArrayOf(2))

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

        assertEquals(listOf(0, LoadingState.Loaded(JsWidget.Spacer())), listOf(registerRender.subscriptionCount.value, viewModel.state.value))
    }

    /** The screen shows the same card again on every return to the chat, and that must not draw it again. */
    @Test
    fun `showing the same card again keeps its render`() = runTest(testDispatcher) {
        val worker = mockk<ProductWorker>(relaxed = true) {
            every { renderRoomFooter(any(), any(), any()) } returns flowOf(Result.success(JsWidget.Spacer()))
        }
        val viewModel = ProductRoomFooterCardViewModel(ROOM, worker)

        viewModel.show(register)
        viewModel.show(ProductRoomFooter.Card("register", byteArrayOf(1)))
        advanceUntilIdle()
        viewModel.handleUiEvent("join", JsUiEvent.Type.ButtonClick)

        verify(exactly = 1) { worker.renderRoomFooter(ProductChatIdParameter(ROOM), "register", any()) }
        verify { worker.dispatchRoomFooterEvent(ProductChatIdParameter(ROOM), "register", "join", JsUiEvent.Type.ButtonClick) }
    }
}
