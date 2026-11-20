package io.paritytech.polkadotapp.feature_products_impl.presentation

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.collections.immutable.persistentSetOf
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
import uniffi.truapi.MaterialFailure
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ProductId
import uniffi.truapi.ResolvedMaterials

class ProductsMessageViewModelMaterialsTest {
    private val testDispatcher = StandardTestDispatcher()
    private val product = Product(ProductId.parse("dim2.dot").getOrThrow(), "Jollity", icon = null)
    private val widget = JsWidget.Materials(index = 0, children = emptyList())
    private val ready = ResolvedMaterials(nodes = listOf(NodeMaterials.Ready(layers = emptyList())), noiseSeed = 7u)

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `resolved materials reach the card, and a node the device failed is reported once and drawn plain`() = runTest(testDispatcher) {
        val resolution = resolutionOf(ready)
        val viewModel = viewModelFor(workerWith(MutableStateFlow(resolution)))
        advanceUntilIdle()

        val drawn = viewModel.state.value
        viewModel.reportMaterial(resolution, 0, byteArrayOf(1), MaterialFailure.UNSUPPORTED)
        viewModel.reportMaterial(resolution, 0, byteArrayOf(1), MaterialFailure.UNSUPPORTED)
        advanceUntilIdle()

        assertEquals(LoadingState.Loaded(ProductCard(widget, ProductCardMaterials(resolution, ready, persistentSetOf()))), drawn)
        assertEquals(LoadingState.Loaded(ProductCard(widget, ProductCardMaterials(resolution, ready, persistentSetOf(0)))), viewModel.state.value)
        verify(exactly = 1) { resolution.report(0u, any(), MaterialFailure.UNSUPPORTED) }
    }

    @Test
    fun `a report against the tree a new one replaced reaches no resolution, and the new tree starts unreported`() = runTest(testDispatcher) {
        val first = resolutionOf(ready)
        val second = resolutionOf(ready)
        val resolutions = MutableStateFlow<MaterialsResolution?>(first)
        val viewModel = viewModelFor(workerWith(resolutions))
        advanceUntilIdle()
        viewModel.reportMaterial(first, 0, byteArrayOf(1), MaterialFailure.UNSUPPORTED)
        resolutions.value = second
        advanceUntilIdle()

        val replaced = viewModel.state.value
        viewModel.reportMaterial(first, 1, byteArrayOf(1), MaterialFailure.UNSUPPORTED)
        viewModel.reportMaterial(second, 0, byteArrayOf(1), MaterialFailure.UNSUPPORTED)
        advanceUntilIdle()

        assertEquals(LoadingState.Loaded(ProductCard(widget, ProductCardMaterials(second, ready, persistentSetOf()))), replaced)
        assertEquals(LoadingState.Loaded(ProductCard(widget, ProductCardMaterials(second, ready, persistentSetOf(0)))), viewModel.state.value)
        verify(exactly = 1) { first.report(any(), any(), any()) }
        verify(exactly = 1) { second.report(0u, any(), MaterialFailure.UNSUPPORTED) }
    }

    private fun resolutionOf(resolved: ResolvedMaterials): MaterialsResolution {
        val resolution: MaterialsResolution = mockk(relaxed = true)
        every { resolution.resolved } returns MutableStateFlow(resolved)
        return resolution
    }

    private fun workerWith(resolutions: MutableStateFlow<MaterialsResolution?>): ProductWorker {
        val worker: ProductWorker = mockk()
        every { worker.renderMessage(any(), any(), any(), any()) } returns flowOf(Result.success(widget))
        every { worker.messageMaterials(any(), any(), any()) } returns resolutions
        return worker
    }

    private fun viewModelFor(worker: ProductWorker) = ProductsMessageViewModel(
        content = ProductsMessageContent("card", ByteArray(0).toDataByteArray()),
        messageId = "m1",
        worker = worker,
        product = product,
        roomId = "room",
        renderObservers = emptySet(),
    )
}
