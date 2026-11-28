package io.paritytech.polkadotapp.feature_products_impl.presentation

import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
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
import uniffi.truapi.ProductId

class ProductsMessageViewModelMaterialsTest {
    private val testDispatcher = StandardTestDispatcher()
    private val product = Product(ProductId.parse("dim2.dot").getOrThrow(), "Jollity", icon = null)

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    // A card drawing the materials of a tree the product replaced would number its nodes against the wrong tree.
    @Test
    fun `the card follows the materials the worker publishes for its message`() = runTest(testDispatcher) {
        val first: MaterialsResolution = mockk()
        val second: MaterialsResolution = mockk()
        val published = MutableStateFlow<MaterialsResolution?>(first)
        val worker: ProductWorker = mockk {
            every { renderMessage(any(), any(), any(), any()) } returns flowOf(Result.success(JsWidget.Materials(node = 0, children = emptyList())))
            every { messageMaterials(ProductChatIdParameter("room"), "m1", "card") } returns published
        }
        val viewModel = viewModelFor(worker)

        val shown = buildList {
            advanceUntilIdle()
            add(viewModel.resolution.value)
            published.value = second
            advanceUntilIdle()
            add(viewModel.resolution.value)
            published.value = null
            advanceUntilIdle()
            add(viewModel.resolution.value)
        }

        assertEquals(listOf(first, second, null), shown)
    }

    private fun viewModelFor(worker: ProductWorker) = ProductsMessageViewModel(
        content = ProductsMessageContent("card", ByteArray(0).toDataByteArray()),
        messageId = "m1",
        worker = worker,
        product = product,
        roomId = "room",
        imageResolver = JsImageResolver { null },
        materialsGl = mockk(),
        materialImages = mockk(),
        renderObservers = emptySet(),
    )
}
