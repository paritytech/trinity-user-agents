package io.paritytech.polkadotapp.app

import android.app.Instrumentation
import android.content.Context
import android.os.Build
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.mockk.coEvery
import io.mockk.mockk
import io.parity.truapi.HostBridge
import io.parity.truapi.LocalhostBridgeBootstrap
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.WebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.io.path.createTempDirectory

@RunWith(AndroidJUnit4::class)
class RendererLossRecoveryTest {
    // A renderer can die while the product sits in the background. Android never revives that
    // WebView, so the product only comes back if a fresh one loads the route it was on with the
    // same bootstrap and reconnects to the execution it already has.
    @Test
    fun productReconnectsFromAFreshWebViewAfterItsRendererDies() {
        assumeTrue(Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q)
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
        val reports = Reports()
        val bridge = mockk<HostBridge>(relaxed = true) { coEvery { coreStorage.read(any()) } returns null }
        TrUAPIHostRuntime(bridge, CONFIG).use { runtime ->
            runtime.openProductExecution(bridge, ProductExecutionConfig(PRODUCT, ProductExecutionKind.APP)).use { execution ->
                val endpoint = execution.startWsBridge()
                val provider = PageProvider(context, scope)
                try {
                    val first = runBlocking {
                        provider.addWebViewSetup { it.addJavascriptInterface(reports, "report") }
                        provider.addWebViewSetup(
                            TrUAPIBootstrapInstaller(context).installerFor(setOf(ORIGIN))(
                                LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token),
                            ),
                        )
                        provider.accessWebView { it.apply { loadUrl(PAGE_URL) } }
                    }
                    val beforeLoss = reports.statuses.poll(15, TimeUnit.SECONDS)
                    instrumentation.runOnMainSync { first.evaluateJavascript("history.pushState({}, '', '/route')", null) }
                    instrumentation.awaitUrl(first, "$ORIGIN/route")
                    instrumentation.runOnMainSync { checkNotNull(first.webViewRenderProcess).terminate() }
                    val afterLoss = reports.statuses.poll(15, TimeUnit.SECONDS)

                    assertEquals(listOf("connected /", "connected /route"), listOf(beforeLoss, afterLoss))
                    assertNotSame(first, provider.getWebViewOrNull())
                } finally {
                    instrumentation.runOnMainSync { provider.getWebViewOrNull()?.destroy() }
                    scope.cancel()
                }
            }
        }
    }

    // Serves one page and reacts to a renderer loss the way the product browser does.
    private class PageProvider(
        private val context: Context,
        private val scope: CoroutineScope,
    ) : WebViewProvider(RealCoroutineDispatchers()) {
        override val callingProductIdProvider = FixedProductId(ProductId.fromStoredValue(PRODUCT))

        override suspend fun createWebView() = WebView(context).apply {
            settings.javaScriptEnabled = true
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest) =
                    WebResourceResponse("text/html", "UTF-8", PAGE.byteInputStream())

                override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail): Boolean {
                    replaceDeadWebView(view, scope, PAGE_URL)
                    return true
                }
            }
        }

        override suspend fun loadInitialContent() = Unit
    }

    private class Reports {
        val statuses = LinkedBlockingQueue<String>()
        @JavascriptInterface fun status(value: String) { statuses.add(value) }
    }

    companion object {
        private fun Instrumentation.awaitUrl(webView: WebView, expected: String) {
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
            var url: String? = null
            while (url != expected) {
                check(System.nanoTime() < deadline) { "WebView stayed on $url" }
                runOnMainSync { url = webView.url }
                Thread.sleep(20)
            }
        }

        private const val PRODUCT = "recovery.paseo"
        private const val ORIGIN = "http://localhost"
        private const val PAGE_URL = "$ORIGIN/"
        private val CONFIG = HostRuntimeConfig(
            hostName = "Renderer loss test",
            peopleChainGenesisHash = ByteArray(32),
            bulletinChainGenesisHash = ByteArray(32),
            assetHubChainGenesisHash = ByteArray(32),
            networkSuffix = "paseo",
            databaseDirectory = createTempDirectory("truapi").toString(),
        )

        // Reading the client is what dials the host, so each fresh page reports its own connection.
        private const val PAGE = """
            <!doctype html><html><script>
            const host = window.__HOST_API_CLIENT__;
            host.subscribeConnectionStatus(status => {
              if (status === 'connected') report.status(status + ' ' + location.pathname);
            });
            host.client;
            </script></html>
        """
    }
}
