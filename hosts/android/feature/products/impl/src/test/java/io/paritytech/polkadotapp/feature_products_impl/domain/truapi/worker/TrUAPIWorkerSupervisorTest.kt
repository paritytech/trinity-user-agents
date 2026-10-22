package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.parity.truapi.ChatHostBridge
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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIWorkerSupervisorTest {
    private val rendererGoneListeners = mutableMapOf<ChatWebViewProvider, MutableList<() -> Unit>>()
    private val providers = mutableListOf<ChatWebViewProvider>()
    private val executions = mutableListOf<TrUAPIProductExecution>()
    private val chatBridges = mutableListOf<ChatHostBridge?>()
    private var includesChat = true
    private var pagesThatNeverFinish = 0

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

    // When the renderer stops before the page is ready, the boot waits for a page that does not come. The core
    // still wants the worker and sends no new start, so only the supervisor can boot it again.
    @Test
    fun `a chat product worker whose renderer died during the boot boots again`() = runTest {
        pagesThatNeverFinish = 1
        val supervisor = supervisor()

        supervisor.onDemandChanged(CHAT_PRODUCT, WorkerDemand.START)
        runCurrent()
        rendererGoneListeners.getValue(providers.single()).forEach { it() }
        advanceUntilIdle()

        assertEquals(2, providers.size)
        assertEquals(executions[1], supervisor.currentExecution(CHAT_PRODUCT))
    }

    @Test
    fun `another product's boot runs on to its own end when its renderer dies, as before the chat product`() = runTest {
        pagesThatNeverFinish = 1
        val supervisor = supervisor()

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        runCurrent()
        rendererGoneListeners.getValue(providers.single()).forEach { it() }
        runCurrent()

        assertEquals(1, providers.size)
    }

    @Test
    fun `a chat product renderer that dies during each boot ends as a failed start`() = runTest {
        pagesThatNeverFinish = Int.MAX_VALUE
        val supervisor = supervisor()

        supervisor.onDemandChanged(CHAT_PRODUCT, WorkerDemand.START)
        repeat(3) {
            runCurrent()
            rendererGoneListeners.getValue(providers.last()).forEach { it() }
        }
        runCurrent()

        assertEquals(3, providers.size)
        assertTrue(supervisor.executionState(CHAT_PRODUCT).first() is WorkerExecutionState.Failed)
    }

    @Test
    fun `only a product that includes chat gets a chat bridge`() = runTest {
        includesChat = false
        val supervisor = supervisor()

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        assertEquals(listOf<ChatHostBridge?>(null), chatBridges)
    }

    @Test
    fun `a chat product gets a chat bridge`() = runTest {
        val supervisor = supervisor()

        supervisor.onDemandChanged(PRODUCT, WorkerDemand.START)
        advanceUntilIdle()

        assertEquals(1, chatBridges.filterNotNull().size)
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
            override suspend fun resolveWorker(productId: ProductId) = Result.success(
                mockk<ProductExecutable.Worker> {
                    every { scriptUrl } returns SCRIPT_URL
                    every { includesChat } returns this@TrUAPIWorkerSupervisorTest.includesChat
                },
            )
        },
        webViewProviderFactory = object : ChatWebViewProvider.Factory {
            override fun create(config: ChatWebViewConfig, scope: CoroutineScope) = provider()
        },
        bootstrapInstaller = mockk(relaxed = true),
        refCounter = { mockk(relaxed = true) },
        dispatchers = testDispatchers(),
    )

    private fun bridge(): ProductTrUAPIHostBridge = mockk {
        val execution = mockk<TrUAPIProductExecution>().also { executions += it }
        coEvery { attach(any(), any(), any(), any(), any(), any(), any(), any()) } answers {
            chatBridges += arg<ChatHostBridge?>(5)
            Result.success(execution)
        }
    }

    // Stands in for the hidden WebView: its page reports finished once loaded, and a renderer
    // loss reaches whoever listens for it.
    private fun provider(): ChatWebViewProvider = mockk<ChatWebViewProvider>(relaxed = true).also { provider ->
        val pageFinished = mutableListOf<() -> Unit>()
        every { provider.addOnPageFinishedListener(any()) } answers { pageFinished += firstArg<() -> Unit>() }
        every { provider.addOnWebViewDestroyedListener(any()) } answers {
            rendererGoneListeners.getOrPut(provider) { mutableListOf() } += firstArg<() -> Unit>()
        }
        val finishes = pagesThatNeverFinish-- <= 0
        coEvery { provider.loadInitialContent() } answers { if (finishes) pageFinished.forEach { it() } }
        coEvery { provider.accessWebView<Any?>(any()) } returns Unit
        providers += provider
    }

    private companion object {
        val PRODUCT = ProductId.fromStoredValue("worker.paseo")
        val CHAT_PRODUCT = ProductId.fromStoredValue("chat.paseo")
        const val SCRIPT_URL = "https://worker.paseo/worker.js"
    }
}
