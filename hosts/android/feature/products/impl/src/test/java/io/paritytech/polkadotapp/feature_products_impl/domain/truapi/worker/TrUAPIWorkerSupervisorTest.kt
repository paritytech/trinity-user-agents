package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIWorkerSupervisorTest {
    private val rendererGoneListeners = mutableMapOf<ChatWebViewProvider, MutableList<() -> Unit>>()
    private val providers = mutableListOf<ChatWebViewProvider>()
    private val executions = mutableListOf<TrUAPIProductExecution>()

    @Before
    fun parseScriptUrls() {
        mockkStatic(Uri::class)
        every { Uri.parse(SCRIPT_URL) } returns mockk {
            every { scheme } returns "https"
            every { host } returns "worker.paseo"
            every { port } returns -1
            every { path } returns "/worker.js"
        }
    }

    @After
    fun restoreUri() = unmockkStatic(Uri::class)

    // A dead renderer takes the worker's running script with it, and the core still wants the worker,
    // so nothing else would ever bring it back: its card would sit on a dead execution.
    @Test
    fun `a running worker whose renderer died boots again`() = runTest {
        val supervisor = supervisor()
        val seen = mutableListOf<TrUAPIProductExecution?>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { supervisor.execution(PRODUCT).toList(seen) }

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()
        rendererGoneListeners.getValue(providers.single()).forEach { it() }
        advanceUntilIdle()

        assertEquals(listOf(null, executions[0], null, executions[1]), seen)
    }

    @Test
    fun `unlock reopens only the worker claims retained while locked`() = runTest {
        val supervisor = supervisor()
        val seen = mutableListOf<TrUAPIProductExecution?>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { supervisor.execution(PRODUCT).toList(seen) }

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()
        supervisor.pause()
        supervisor.resume()
        advanceUntilIdle()
        supervisor.pause()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.STOP)
        advanceUntilIdle()
        supervisor.resume()
        advanceUntilIdle()

        assertEquals(listOf(null, executions[0], null, executions[1], null), seen)
    }

    @Test
    fun `an old runtime stop cannot dispose the new wallet worker`() = runTest {
        val supervisor = supervisor()
        var generation = 1
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START) { generation == 1 }
        advanceUntilIdle()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.STOP) { generation == 1 }
        generation = 2
        supervisor.stopAll()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START) { generation == 2 }
        advanceUntilIdle()
        supervisor.onDemandChanged(PRODUCT, WorkerDemand.STOP) { generation == 1 }
        advanceUntilIdle()

        assertEquals(executions.last(), supervisor.currentExecution(PRODUCT))
    }

    private fun TestScope.supervisor() = TrUAPIWorkerSupervisor(
        runtimeProvider = {
            mockk<TrUAPIHostRuntimeProvider> { coEvery { runtime() } returns Result.success(mockk()) }
        },
        hostBridgeFactory = object : ProductTrUAPIHostBridge.Factory {
            override fun create(scope: CoroutineScope) = bridge()
        },
        chainDirectory = mockk { coEvery { resolve() } returns mockk() },
        scriptResolver = object : ProductScriptResolver {
            override suspend fun resolveWorker(productId: ProductId) =
                Result.success(mockk<ProductExecutable.Worker> { every { scriptUrl } returns SCRIPT_URL })
        },
        webViewProviderFactory = object : ChatWebViewProvider.Factory {
            override fun create(config: ChatWebViewConfig, scope: CoroutineScope) = provider()
        },
        bootstrapInstaller = mockk(relaxed = true),
        dispatchers = testDispatchers(),
    )

    private fun bridge(): ProductTrUAPIHostBridge = mockk {
        val execution = mockk<TrUAPIProductExecution>().also { executions += it }
        coEvery { attach(any(), any(), any(), any(), any(), any(), any()) } returns Result.success(execution)
    }

    // Stands in for the hidden WebView: its page reports finished once loaded, and a renderer
    // loss reaches whoever listens for it.
    private fun provider(): ChatWebViewProvider = mockk<ChatWebViewProvider>(relaxed = true).also { provider ->
        val pageFinished = mutableListOf<() -> Unit>()
        every { provider.addOnPageFinishedListener(any()) } answers { pageFinished += firstArg<() -> Unit>() }
        every { provider.addOnWebViewDestroyedListener(any()) } answers {
            rendererGoneListeners.getOrPut(provider) { mutableListOf() } += firstArg<() -> Unit>()
        }
        coEvery { provider.loadInitialContent() } answers { pageFinished.forEach { it() } }
        coEvery { provider.accessWebView<Any?>(any()) } returns Unit
        providers += provider
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("worker.paseo")
        const val SCRIPT_URL = "https://worker.paseo/worker.js"
    }
}
