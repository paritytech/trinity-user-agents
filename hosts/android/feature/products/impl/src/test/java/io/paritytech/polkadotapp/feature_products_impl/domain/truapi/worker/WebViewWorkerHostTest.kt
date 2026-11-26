package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import android.webkit.ValueCallback
import android.webkit.WebView
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.slot
import io.mockk.unmockkStatic
import io.mockk.verify
import io.parity.truapi.ChatHostBridge
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.webView.WebViewJsBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.isActive
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class WebViewWorkerHostTest {
    private val attempt = mockk<NativeWorkerAttempt>(relaxUnitFun = true) {
        every { id() } returns 1uL
        every { moduleStarted() } returns true
    }
    private val rendererGoneListeners = mutableMapOf<ChatWebViewProvider, MutableList<() -> Unit>>()
    private val providers = mutableListOf<ChatWebViewProvider>()
    private val chatBridges = mutableListOf<ChatHostBridge?>()
    private val bootstrapInstaller = mockk<TrUAPIBootstrapInstaller>(relaxed = true)
    private val policies = mutableListOf<NavigationPolicy>()
    private val workerPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {})
    private val workerScopes = mutableListOf<CoroutineScope>()
    private val policyScopes = mutableListOf<CoroutineScope>()
    private var includesChat = true

    private var onModuleLoad: ((String) -> Unit) -> Unit = { complete -> complete("") }

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

    @Test
    fun `a renderer loss reports the acknowledged attempt to the core`() = runTest {
        val host = createHost()
        host.start(PRODUCT, attempt)
        advanceUntilIdle()
        rendererGoneListeners.getValue(providers.single()).forEach { it() }
        advanceUntilIdle()

        verify(exactly = 1) { attempt.moduleStarted() }
        verify(exactly = 1) { attempt.failed("WebView renderer exited") }
        assertEquals(1, providers.size)
    }

    @Test
    fun `a delayed stop disposes only its own WebView`() = runTest {
        val replacement = mockk<NativeWorkerAttempt> {
            every { id() } returns 2uL
            every { moduleStarted() } returns true
        }
        val host = createHost()
        host.start(PRODUCT, attempt)
        host.start(PRODUCT, replacement)
        advanceUntilIdle()
        host.stop(PRODUCT, attempt.id())
        advanceUntilIdle()

        assertEquals(listOf(false, true), workerScopes.map { it.isActive })
        verify(exactly = 1) { replacement.moduleStarted() }
    }

    @Test
    fun `an open bridge stays pending until the worker module has loaded`() = runTest {
        lateinit var finish: (String) -> Unit
        onModuleLoad = { finish = it }
        createHost().start(PRODUCT, attempt)
        runCurrent()

        verify(exactly = 0) { attempt.moduleStarted() }
        finish("worker import rejected")
        runCurrent()

        verify(exactly = 1) { attempt.failed("worker import rejected") }
        verify(exactly = 0) { attempt.moduleStarted() }
    }

    @Test
    fun `only a product that includes chat gets a chat bridge`() = runTest {
        includesChat = false
        val host = createHost()

        host.start(PRODUCT, attempt)
        advanceUntilIdle()

        assertEquals(listOf<ChatHostBridge?>(null), chatBridges)
    }

    @Test
    fun `a chat product gets a chat bridge`() = runTest {
        val host = createHost()

        host.start(PRODUCT, attempt)
        advanceUntilIdle()

        assertEquals(1, chatBridges.filterNotNull().size)
    }

    @Test
    fun `a worker bootstrap does not install host WebView telemetry`() = runTest {
        val host = createHost()

        host.start(PRODUCT, attempt)
        advanceUntilIdle()

        verify(exactly = 1) { bootstrapInstaller.installerFor(setOf("https://worker.paseo"), null) }
    }

    // A chat card button navigates from the worker, so the worker's execution needs a policy that
    // opens the destination rather than one that drops it, on the worker's own scope so that
    // stopping the worker cancels a navigation still in flight.
    @Test
    fun `a worker navigates through the chat worker navigation`() = runTest {
        val host = createHost()

        host.start(PRODUCT, attempt)
        advanceUntilIdle()

        assertEquals(listOf(workerPolicy) to workerScopes, policies to policyScopes)
    }

    private fun TestScope.createHost() = WebViewWorkerHost(
        hostBridgeFactory = object : ProductTrUAPIHostBridge.Factory {
            override fun create(scope: CoroutineScope) = bridge()
        },
        chainDirectory = mockk { coEvery { resolve() } returns mockk() },
        scriptResolver = object : ProductScriptResolver {
            override suspend fun resolveWorker(productId: ProductId) = Result.success(
                mockk<ProductExecutable.Worker> {
                    every { scriptUrl } returns SCRIPT_URL
                    every { includesChat } returns this@WebViewWorkerHostTest.includesChat
                },
            )
        },
        webViewProviderFactory = object : ChatWebViewProvider.Factory {
            override fun create(config: ChatWebViewConfig, scope: CoroutineScope) = provider().also { workerScopes += scope }
        },
        bootstrapInstaller = bootstrapInstaller,
        refCounter = { mockk(relaxed = true) },
        navigation = mockk { every { policy(PRODUCT, capture(policyScopes)) } returns workerPolicy },
        dispatchers = testDispatchers(),
    )

    private fun bridge(): ProductTrUAPIHostBridge = mockk {
        val execution = mockk<TrUAPIProductExecution>()
        coEvery { attachWorker(any(), any(), any(), any(), any(), any()) } answers {
            policies += arg<NavigationPolicy>(3)
            chatBridges += arg<ChatHostBridge?>(4)
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
        coEvery { provider.loadInitialContent() } answers { pageFinished.forEach { it() } }
        val bridge = slot<WebViewJsBridge>()
        val view = mockk<WebView>(relaxed = true)
        every { provider.getWebViewOrNull() } returns null
        every { view.addJavascriptInterface(capture(bridge), "Android") } returns Unit
        every { view.evaluateJavascript(any(), any()) } answers {
            val script = firstArg<String>()
            val callback = requireNotNull(Regex("worker-module-[a-f0-9-]+").find(script)).value
            onModuleLoad { error -> bridge.captured.call(callback, error) }
            secondArg<ValueCallback<String>>().onReceiveValue("null")
        }
        coEvery { provider.addWebViewSetup(any()) } answers { firstArg<(WebView) -> Unit>()(view) }
        coEvery { provider.accessWebView<Any?>(any()) } coAnswers { firstArg<suspend (WebView) -> Any?>()(view) }
        providers += provider
    }

    private companion object {
        val PRODUCT = ProductId.parse("worker.paseo").getOrThrow()
        const val SCRIPT_URL = "https://worker.paseo/worker.js"
    }
}
