package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.net.Uri
import android.webkit.WebView
import io.mockk.coAnswers
import io.mockk.coEvery
import io.mockk.coJustRun
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.runs
import io.mockk.slot
import io.mockk.unmockkStatic
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.ProductExecutionKind

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPISessionStarterTest {
    private val bridge = mockk<ProductTrUAPIHostBridge>()
    private val runtime = mockk<TrUAPIHostRuntime>()
    private val chains = mockk<TrUAPIChains>()
    private val navigation = mockk<NavigationPolicy>()
    private val provider = mockk<BrowserWebViewProvider>()
    private val bootstrapInstaller = mockk<TrUAPIBootstrapInstaller>()
    private val telemetry = mockk<HostWebViewCreatedTelemetry>()
    private val visibleProducts = mockk<VisibleProducts>(relaxed = true)
    private val webViewSetups = mutableListOf<(WebView) -> Unit>()
    private val installerSessionId = slot<String>()
    private val reportedSessionId = slot<String>()

    private val starter = TrUAPISessionStarter(
        hostBridgeFactory = mockk { every { create(any()) } returns bridge },
        runtimeProvider = mockk { coEvery { runtime() } returns Result.success(runtime) },
        chainDirectory = mockk { coEvery { resolve() } returns chains },
        dotNsTldProvider = mockk(),
        bootstrapInstaller = bootstrapInstaller,
        visibleProducts = visibleProducts,
        hostWebViewCreatedTelemetry = telemetry,
    )

    @Before
    fun setUp() {
        mockkStatic(Uri::class)
        every { Uri.parse(PRODUCT_URL) } returns mockk {
            every { scheme } returns "https"
            every { host } returns "product.paseo"
            every { port } returns -1
        }

        every { bootstrapInstaller.installerFor(any(), capture(installerSessionId)) } returns { _ ->
            { _ -> Unit }
        }
        every { telemetry.report(capture(reportedSessionId)) } just runs
        coEvery { provider.addWebViewSetup(any()) } answers {
            webViewSetups += firstArg<(WebView) -> Unit>()
        }
        every { provider.useTrUAPIPermissions(PRODUCT_ID, any()) } just runs
        coJustRun { provider.loadInitialContent() }
        coEvery {
            bridge.attach(
                runtime = runtime,
                productId = PRODUCT_ID,
                chains = chains,
                navigationPolicy = navigation,
                kind = ProductExecutionKind.APP,
                chat = null,
                card = null,
                onReadyToInject = any(),
            )
        } coAnswers {
            arg<suspend (String) -> Unit>(7)("endpoint-bootstrap")
            Result.success(mockk<TrUAPIProductExecution>())
        }
    }

    @After
    fun tearDown() = unmockkStatic(Uri::class)

    @Test
    fun `the first WebView setup reports the same session id exposed to the product`() = runTest {
        starter.start(
            provider = provider,
            productUrl = PRODUCT_URL,
            scope = this,
            hostApiNavigation = navigation,
            explicitProductId = PRODUCT_ID,
        )
        advanceUntilIdle()

        val telemetrySetup = webViewSetups.last()
        telemetrySetup(mockk())
        telemetrySetup(mockk())

        verify(exactly = 1) { telemetry.report(any()) }
        assertEquals(installerSessionId.captured, reportedSessionId.captured)
    }

    private companion object {
        const val PRODUCT_URL = "https://product.paseo/app"
        val PRODUCT_ID = ProductId.fromStoredValue("product.paseo")
    }
}
