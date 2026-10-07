package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.RuntimeState
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.truapi.WsBridgeEndpoint

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIWorkerSupervisorTest {
    private val rendererGoneListeners = mutableMapOf<ChatWebViewProvider, MutableList<() -> Unit>>()
    private val providers = mutableListOf<ChatWebViewProvider>()
    private val executions = mutableListOf<TrUAPIProductExecution>()
    private var wasmiEnabled = false
    private val wasmiRuntimes = mutableListOf<WasmiWorkerRuntime>()
    private val wasmiStates = mutableListOf<MutableStateFlow<RuntimeState>>()

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

    // The wasm path opens the same execution and bridge, then hands the module to the sandbox runtime
    // instead of a WebView. A guest fault ends the worker once; nothing reboots it.
    @Test
    fun `with the wasmi setting on, a worker runs in the sandbox and a fault stops it`() = runTest {
        wasmiEnabled = true
        val supervisor = supervisor()
        val seen = mutableListOf<TrUAPIProductExecution?>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { supervisor.execution(PRODUCT).toList(seen) }

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        assertTrue("no WebView is created for a wasm worker", providers.isEmpty())
        assertEquals(1, wasmiRuntimes.size)
        assertEquals(listOf(null, executions[0]), seen)

        wasmiStates.single().value = RuntimeState.Error("ran out of fuel")
        advanceUntilIdle()

        assertEquals(listOf(null, executions[0], null), seen)
        assertEquals(1, wasmiRuntimes.size)
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
        runtimeSettings = mockk<ProductRuntimeSettings> { every { isWasmiWorkerRuntimeEnabled() } answers { wasmiEnabled } },
        wasmiRuntimeFactory = object : WasmiWorkerRuntime.Factory {
            override fun create(scope: CoroutineScope) = wasmiRuntime()
        },
        guestFactory = mockk { every { create(any(), any(), any()) } returns Result.success(mockk(relaxed = true)) },
        moduleFetcher = mockk { coEvery { fetch(any()) } returns Result.success(WASM_HEADER) },
        dispatchers = testDispatchers(),
    )

    private fun wasmiRuntime(): WasmiWorkerRuntime {
        val state = MutableStateFlow<RuntimeState>(RuntimeState.NotInitialized).also { wasmiStates += it }
        return mockk<WasmiWorkerRuntime>(relaxed = true) {
            every { this@mockk.state } returns state
            coEvery { start(any(), any()) } answers {
                state.value = RuntimeState.Ready
                Result.success(Unit)
            }
        }.also { wasmiRuntimes += it }
    }

    private fun bridge(): ProductTrUAPIHostBridge = mockk {
        val execution = mockk<TrUAPIProductExecution>().also { executions += it }
        coEvery { attach(any(), any(), any(), any(), any(), any()) } returns Result.success(execution)
        every { bridgeEndpoint } returns WsBridgeEndpoint(port = 9731u, token = "t")
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
        val WASM_HEADER = byteArrayOf(0x00, 0x61, 0x73, 0x6D, 1, 0, 0, 0)
    }
}
