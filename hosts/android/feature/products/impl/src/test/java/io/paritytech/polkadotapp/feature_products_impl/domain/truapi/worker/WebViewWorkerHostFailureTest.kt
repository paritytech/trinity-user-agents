package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Test
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class WebViewWorkerHostFailureTest {
    private val productId = ProductId.parse("chat.dot").getOrThrow()
    private val attempt = mockk<NativeWorkerAttempt>(relaxUnitFun = true) { every { id() } returns 1uL }
    private val resolver = mockk<ProductScriptResolver>()

    @Test
    fun `script resolution failure is reported to the attempt that requested it`() = runTest {
        coEvery { resolver.resolveWorker(productId) } returns Result.failure(IllegalStateException("unavailable"))

        createHost().start(productId, attempt)
        advanceUntilIdle()

        verify(exactly = 1) { attempt.failed("unavailable") }
    }

    @Test
    fun `stopping a pending WebView does not report a boot failure`() = runTest {
        val gate = CompletableDeferred<Unit>()
        coEvery { resolver.resolveWorker(productId) } coAnswers {
            gate.await()
            error("resolution cancelled")
        }
        val host = createHost()

        host.start(productId, attempt)
        advanceUntilIdle()
        host.stop(productId, attempt.id())
        advanceUntilIdle()

        verify(exactly = 0) { attempt.failed(any()) }
    }

    private fun TestScope.createHost() = WebViewWorkerHost(
        hostBridgeFactory = mockk(),
        chainDirectory = mockk(),
        scriptResolver = resolver,
        webViewProviderFactory = mockk(),
        bootstrapInstaller = mockk(),
        refCounter = { mockk() },
        navigation = mockk(),
        dispatchers = testDispatchers(),
    )
}
