package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.net.Uri
import android.webkit.WebView
import io.mockk.Runs
import io.mockk.coEvery
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.slot
import io.mockk.unmockkStatic
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.ProductId

class TrUAPISessionStarterTest {
    private val runtime = mockk<TrUAPIHostRuntime>()
    private val execution = mockk<TrUAPIProductExecution>()
    private val bridge = mockk<ProductTrUAPIHostBridge>()
    private val runtimeProvider = mockk<TrUAPIHostRuntimeProvider>()
    private val chains = mockk<TrUAPIChains>()
    private val navigation = NavigationPolicy.DeeplinkNavigation { }
    private val provider = mockk<BrowserWebViewProvider>()
    private val bootstrapInstaller = mockk<TrUAPIBootstrapInstaller>()
    private val telemetry = mockk<HostWebViewCreatedTelemetry>()
    private val visibleProducts = mockk<VisibleProducts>(relaxed = true)
    private val hostBridgeFactory = mockk<ProductTrUAPIHostBridge.Factory>()
    private val chainDirectory = mockk<TrUAPIChainDirectory>()
    private val dotNsTldProvider = mockk<DotNsTldProvider>()
    private val installerSessionId = slot<String>()
    private val reportedSessionId = slot<String>()
    private val webViewSetups = mutableListOf<(WebView) -> Unit>()

    private val starter = TrUAPISessionStarter(
        hostBridgeFactory = hostBridgeFactory,
        runtimeProvider = runtimeProvider,
        chainDirectory = chainDirectory,
        dotNsTldProvider = dotNsTldProvider,
        bootstrapInstaller = bootstrapInstaller,
        visibleProducts = visibleProducts,
        hostWebViewCreatedTelemetry = telemetry,
    )

    @Before
    fun setUp() = runBlocking<Unit> {
        stubProductUrl()
        stubSuccessfulSession()
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
    }

    @Test
    fun `the first WebView setup reports the same session id exposed to the product`() = runBlocking<Unit> {
        starter.start(
            provider = provider,
            productUrl = PRODUCT_URL,
            scope = this,
            hostApiNavigation = navigation,
            explicitProductId = PRODUCT_ID,
        )
        yield()

        invokeTelemetrySetupTwice()

        verify(exactly = 1) { telemetry.report(any()) }
        assertEquals(installerSessionId.captured, reportedSessionId.captured)
    }

    private fun stubProductUrl() {
        val uri = mockk<Uri> {
            every { scheme } returns "https"
            every { host } returns "product.paseo"
            every { port } returns -1
        }
        mockkStatic(Uri::class)
        every { Uri.parse(PRODUCT_URL) } returns uri
    }

    private fun stubSuccessfulSession() {
        every { hostBridgeFactory.create(any()) } returns bridge
        coEvery { runtimeProvider.runtime() } returns Result.success(runtime)
        coEvery { chainDirectory.resolve() } returns chains
        every { bootstrapInstaller.installerFor(any(), capture(installerSessionId)) } returns { { } }
        coEvery { provider.addWebViewSetup(capture(webViewSetups)) } just Runs
        coEvery { provider.loadInitialContent() } just Runs
        every { telemetry.report(capture(reportedSessionId)) } just Runs
        coEvery { bridge.attach(any(), any(), any(), any(), any(), any(), any(), any()) } coAnswers {
            arg<suspend (String) -> Unit>(7)("endpoint-bootstrap")
            Result.success(execution)
        }
    }

    private fun invokeTelemetrySetupTwice() {
        val telemetrySetup = webViewSetups.last()
        telemetrySetup(mockk())
        telemetrySetup(mockk())
    }

    private companion object {
        const val PRODUCT_URL = "https://product.paseo/app"
        val PRODUCT_ID = ProductId.parse("product.paseo").getOrThrow()
    }
}
