package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

private const val ROOM = "jollity"

class ProductRoomFooterWebViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val register = ProductFooterMessage.Action("jollity.register", null)

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `a press that reaches the product confirms the page`() = runTest(testDispatcher) {
        val worker = mockk<ProductWorker> { every { dispatchRoomFooterPageAction(any(), any(), any()) } returns true }
        val viewModel = viewModel(worker)

        viewModel.onPageAction(register)

        assertEquals(ProductFooterHostEvent.Confirmed, viewModel.hostEvents.first())
        verify { worker.dispatchRoomFooterPageAction(ProductChatIdParameter(ROOM), "jollity.register", null) }
    }

    /** A runtime that cannot take the press must send the slider back, not leave it waiting at the end. */
    @Test
    fun `a press that does not reach the product resets the page`() = runTest(testDispatcher) {
        val viewModel = viewModel(RecordingWorker())

        viewModel.onPageAction(register)

        assertEquals(ProductFooterHostEvent.Reset, viewModel.hostEvents.first())
    }

    @Test
    fun `a burst of presses reaches the product once`() = runTest(testDispatcher) {
        val worker = mockk<ProductWorker> { every { dispatchRoomFooterPageAction(any(), any(), any()) } returns true }
        val viewModel = viewModel(worker)

        viewModel.onPageAction(register)
        viewModel.onPageAction(register)
        viewModel.onPageAction(ProductFooterMessage.Action("other", "x"))

        assertEquals(listOf(ProductFooterHostEvent.Confirmed), viewModel.hostEvents.take(1).toList())
        verify(exactly = 1) { worker.dispatchRoomFooterPageAction(any(), any(), any()) }
    }

    private fun viewModel(worker: ProductWorker) =
        ProductRoomFooterWebViewModel(ROOM, worker, mockk(relaxed = true), mockk(relaxed = true), mockk(relaxed = true))
}
