package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.net.Uri
import android.webkit.WebView
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.mockito.MockedStatic
import org.mockito.Mockito.mock
import org.mockito.Mockito.mockStatic
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import uniffi.truapi.ProductExecutionKind

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPISessionStarterTest {
    private val bridge = mock<ProductTrUAPIHostBridge>()
    private val runtime = mock<TrUAPIHostRuntime>()
    private val chains = mock<TrUAPIChains>()
    private val navigation = NavigationPolicy.DeeplinkNavigation { }
    private val provider = mock<BrowserWebViewProvider>()
    private val bootstrapInstaller = mock<TrUAPIBootstrapInstaller>()
    private val telemetry = mock<HostWebViewCreatedTelemetry>()
    private val visibleProducts = mock<VisibleProducts>()
    private val hostBridgeFactory = mock<ProductTrUAPIHostBridge.Factory>()
    private val runtimeProvider = mock<TrUAPIHostRuntimeProvider>()
    private val chainDirectory = mock<TrUAPIChainDirectory>()
    private val dotNsTldProvider = mock<DotNsTldProvider>()
    private val webViewSetups = mutableListOf<(WebView) -> Unit>()
    private var installerSessionId: String? = null
    private var reportedSessionId: String? = null
    private lateinit var uriStatic: MockedStatic<Uri>

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
    fun setUp() {
        stubProductUrl()
        stubSuccessfulSession()
    }

    @After
    fun tearDown() = uriStatic.close()

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

        invokeTelemetrySetupTwice()

        verifyTelemetryReportedOnce()
        assertEquals(installerSessionId, reportedSessionId)
    }

    private fun stubProductUrl() {
        val uri = mock<Uri>()
        uriStatic = mockStatic(Uri::class.java)
        uriStatic.`when`<Uri> { Uri.parse(PRODUCT_URL) }.thenReturn(uri)
        whenever(uri.scheme).thenReturn("https")
        whenever(uri.host).thenReturn("product.paseo")
        whenever(uri.port).thenReturn(-1)
    }

    private fun stubSuccessfulSession() = runBlocking {
        whenever(hostBridgeFactory.create(any())).thenReturn(bridge)
        whenever(runtimeProvider.runtime()).thenReturn(Result.success(runtime))
        whenever(chainDirectory.resolve()).thenReturn(chains)
        whenever(bootstrapInstaller.installerFor(any(), any())).thenAnswer { invocation ->
            installerSessionId = invocation.getArgument(1)
            val installer: (String) -> (WebView) -> Unit = { { Unit } }
            installer
        }
        whenever(provider.addWebViewSetup(any())).thenAnswer { invocation ->
            webViewSetups += invocation.getArgument<(WebView) -> Unit>(0)
            Unit
        }
        whenever(provider.loadInitialContent()).thenReturn(Unit)
        whenever(
            bridge.attach(
                runtime = eq(runtime),
                productId = eq(PRODUCT_ID),
                chains = eq(chains),
                navigationPolicy = eq(navigation),
                kind = eq(ProductExecutionKind.APP),
                chat = eq(null),
                card = eq(null),
                onReadyToInject = any(),
            ),
        ).thenAnswer { invocation ->
            runBlocking { invocation.getArgument<suspend (String) -> Unit>(7)("endpoint-bootstrap") }
            Result.success(mock<TrUAPIProductExecution>())
        }
        whenever(telemetry.report(any())).thenAnswer { invocation ->
            reportedSessionId = invocation.getArgument(0)
            Unit
        }
    }

    private fun invokeTelemetrySetupTwice() {
        val telemetrySetup = webViewSetups.last()
        telemetrySetup(mock())
        telemetrySetup(mock())
    }

    private fun verifyTelemetryReportedOnce() {
        verify(telemetry, times(1)).report(any())
    }

    private companion object {
        const val PRODUCT_URL = "https://product.paseo/app"
        val PRODUCT_ID = ProductId.fromStoredValue("product.paseo")
    }
}
