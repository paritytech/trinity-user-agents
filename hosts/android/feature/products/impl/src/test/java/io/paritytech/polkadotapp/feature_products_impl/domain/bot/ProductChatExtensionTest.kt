package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtensionContext
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.chat.TrUAPIChatHost
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProductChatExtensionTest {
    private val reference = mockk<ProductWorkerReference>(relaxed = true)
    private val workerRefCounter = mockk<ProductWorkerRefCounter> {
        coEvery { acquire(any(), any()) } returns reference
    }
    private val coreChatHost = mockk<TrUAPIChatHost> {
        coEvery { serve(any(), any(), any(), any()) } coAnswers { awaitCancellation() }
    }
    private var wasmiProduct = false

    // Two workers for one product would both answer its chat: the opted-in product must never boot
    // the native one, whose script is not even JavaScript.
    @Test
    fun `an opted-in product's chat is served from the core and never boots the native worker`() = runTest {
        wasmiProduct = true

        startExtension()

        coVerify(exactly = 1) { coreChatHost.serve(PRODUCT.id, any(), any(), any()) }
        coVerify(exactly = 0) { workerRefCounter.acquire(any(), any()) }
    }

    @Test
    fun `every other product's chat keeps its native worker`() = runTest {
        startExtension()

        coVerify(exactly = 1) { workerRefCounter.acquire(PRODUCT.id, any()) }
        coVerify(exactly = 0) { coreChatHost.serve(any(), any(), any(), any()) }
    }

    private fun TestScope.startExtension() {
        val extension = ProductChatExtension(
            appContext = mockk(relaxed = true),
            product = PRODUCT,
            workerRefCounter = workerRefCounter,
            runtimeSettings = mockk<ProductRuntimeSettings> { every { isWasmiWorkerProduct(PRODUCT.id) } answers { wasmiProduct } },
            coreChatHost = coreChatHost,
        )
        val context = mockk<ChatExtensionContext> {
            // A real instance: the value-class wrapper ComputationalScope(...) does not survive a mock answer.
            // Unconfined, because the scheduler does not run backgroundScope work on its own.
            every { scope } returns object :
                ComputationalScope,
                CoroutineScope by CoroutineScope(backgroundScope.coroutineContext + UnconfinedTestDispatcher(testScheduler)) {}
            every { subscribeNewMessages(any(), any()) } returns emptyFlow()
        }

        with(context) { extension.startGlobalWork() }
    }

    private companion object {
        val PRODUCT = Product(id = ProductId.fromStoredValue("counter.paseo"), name = "Counter", icon = null)
    }
}
