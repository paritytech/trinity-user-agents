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
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadFailureTelemetry
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenSource
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpen
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpens
import io.paritytech.polkadotapp.common.data.telemetry.HostPageSession
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
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
    private val installerOpen = slot<HostPageOpen?>()
    private val installerOpens = mutableListOf<HostPageOpen?>()
    private val reportedSession = slot<HostPageSession>()
    private val origin = HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L)
    private val events = mutableListOf<HostEvent>()
    private val pageOpens = HostPageOpens({ origin }, { events += it() }, { NOW_MS })
    private val visibleListeners = mutableListOf<() -> Unit>()
    private val visibleReports = mutableListOf<Triple<String, Long, HostPageOpen?>>()
    private val webViewSetups = mutableListOf<(WebView) -> Unit>()

    private val starter = TrUAPISessionStarter(
        hostBridgeFactory = hostBridgeFactory,
        runtimeProvider = runtimeProvider,
        chainDirectory = chainDirectory,
        dotNsTldProvider = dotNsTldProvider,
        bootstrapInstaller = bootstrapInstaller,
        visibleProducts = visibleProducts,
        hostWebViewCreatedTelemetry = telemetry,
        pageOpens = pageOpens,
        loadFailures = HostLoadFailureTelemetry({ origin }, { events += it() }),
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
        startPage()
        yield()

        invokeTelemetrySetupTwice()

        verify(exactly = 1) { telemetry.report(any()) }
        assertEquals(installerSessionId.captured, reportedSession.captured.id)
    }

    @Test
    fun `the tap that asked for the page is handed to its bootstrap once`() = runBlocking<Unit> {
        val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = NOW_MS, sessionId = "open-session")
        pageOpens.hold(PRODUCT_ID.value, tap)

        repeat(2) {
            startPage()
            yield()
        }

        assertEquals(listOf(tap, null), installerOpens)
    }

    @Test
    fun `the session adopts the id minted at the tap and shows it to the product`() = runBlocking<Unit> {
        val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = NOW_MS, sessionId = "open-session")
        pageOpens.hold(PRODUCT_ID.value, tap)

        startPage()
        yield()
        invokeTelemetrySetupTwice()

        assertEquals(tap.sessionId, installerSessionId.captured)
        assertEquals(tap.sessionId, reportedSession.captured.id)
    }

    @Test
    fun `a session with no held open mints its own id`() = runBlocking<Unit> {
        startPage()
        yield()

        assertTrue(installerSessionId.captured.isNotEmpty())
    }

    @Test
    fun `the page is reported visible once, with the open that asked for it`() = runBlocking<Unit> {
        val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = NOW_MS, sessionId = "open-session")
        pageOpens.hold(PRODUCT_ID.value, tap)
        startPage()
        yield()
        invokeTelemetrySetupTwice()

        repeat(3) { visibleListeners.single()() }

        assertEquals(listOf(Triple(tap.sessionId, CREATED_AT_MS, tap)), visibleReports)
    }

    @Test
    fun `a page nothing asked for is reported visible without an open`() = runBlocking<Unit> {
        startPage()
        yield()
        invokeTelemetrySetupTwice()

        visibleListeners.single()()

        assertEquals(listOf(null), visibleReports.map { it.third })
    }

    @Test
    fun `an open too old to time from still gives the page its id, without a time to load from`() = runBlocking<Unit> {
        val staleTap = HostPageOpen(
            HostOpenSource.POCKET_CARD,
            epochMs = 1_000L,
            monotonicMs = NOW_MS - HostPageOpens.MAX_AGE_MS - 1,
            sessionId = "open-session",
        )
        pageOpens.hold(PRODUCT_ID.value, staleTap)

        startPage()
        yield()

        assertEquals("open-session", installerSessionId.captured)
        assertEquals(listOf(null), installerOpens)
        assertEquals("open-session", events.single { it.name == "host.pocket.opened" }.tags["host.session_id"])
    }

    @Test
    fun `an open that shows the running page again is reported under the session id the page reads`() = runBlocking<Unit> {
        startPage()
        yield()

        pageOpens.hold(PRODUCT_ID.value, HostPageOpen(HostOpenSource.POCKET_CARD, 1_000L, NOW_MS, "reuse-tap"))
        pageOpens.reuse(PRODUCT_ID.value)

        assertEquals("true", events.single { it.name == "host.pocket.opened" }.tags["open.reused"])
        assertEquals(installerSessionId.captured, events.single { it.name == "host.pocket.opened" }.tags["host.session_id"])
    }

    @Test
    fun `a page whose scope ended is no longer the one a reused open is reported under`() = runBlocking<Unit> {
        val pageScope = CoroutineScope(coroutineContext + Job())
        pageScope.startPage()
        yield()
        pageScope.cancel()
        yield()

        pageOpens.hold(PRODUCT_ID.value, HostPageOpen(HostOpenSource.POCKET_CARD, 1_000L, NOW_MS, "reuse-tap"))
        pageOpens.reuse(PRODUCT_ID.value)

        assertEquals("reuse-tap", events.single { it.name == "host.pocket.opened" }.tags["host.session_id"])
    }

    @Test
    fun `a page that ended before it was shown is retried under the same id, timed from the same tap`() = runBlocking<Unit> {
        val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = NOW_MS, sessionId = "open-session")
        pageOpens.hold(PRODUCT_ID.value, tap)
        val firstScope = CoroutineScope(coroutineContext + Job())
        firstScope.startPage()
        yield()
        firstScope.cancel()
        yield()

        startPage()
        yield()
        invokeTelemetrySetupTwice()

        assertEquals(listOf(tap, tap), installerOpens)
        assertEquals("open-session", installerSessionId.captured)
        assertEquals(HostPageSession(PRODUCT_ID.value, "open-session", tap, isRetry = true), reportedSession.captured)
        // The open was reported once, by the session that first took it.
        assertEquals(1, events.count { it.name == "host.pocket.opened" })
    }

    @Test
    fun `a page that was shown leaves nothing for the next session to retry`() = runBlocking<Unit> {
        val tap = HostPageOpen(HostOpenSource.POCKET_CARD, epochMs = 1_000L, monotonicMs = NOW_MS, sessionId = "open-session")
        pageOpens.hold(PRODUCT_ID.value, tap)
        val firstScope = CoroutineScope(coroutineContext + Job())
        firstScope.startPage()
        yield()
        invokeTelemetrySetupTwice()
        visibleListeners.single()()
        firstScope.cancel()
        yield()

        startPage()
        yield()

        assertEquals(listOf(tap, null), installerOpens)
    }

    @Test
    fun `a page that fails to start reports a page load failure under its session`() = runBlocking<Unit> {
        coEvery { runtimeProvider.runtime() } returns Result.failure(IllegalStateException("no runtime"))

        startPage()
        yield()

        val event = events.single()
        assertEquals("page", event.tags["load.stage"])
        assertEquals("product.paseo", event.tags["product.id"])
        assertTrue(event.tags["host.session_id"]?.isNotEmpty() == true)
    }

    private fun CoroutineScope.startPage() {
        starter.start(
            provider = provider,
            productUrl = PRODUCT_URL,
            scope = this,
            hostApiNavigation = navigation,
            explicitProductId = PRODUCT_ID,
        )
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
        every { bootstrapInstaller.installerFor(any(), capture(installerSessionId), captureNullable(installerOpen)) } answers {
            installerOpens += installerOpen.captured
            { { } }
        }
        coEvery { provider.addWebViewSetup(capture(webViewSetups)) } just Runs
        coEvery { provider.loadInitialContent() } just Runs
        every { provider.addOnPageCommitVisibleListener(capture(visibleListeners)) } just Runs
        every { telemetry.webViewCreatedAtMs() } returns CREATED_AT_MS
        every { telemetry.reportVisible(any(), any()) } answers {
            val session = firstArg<HostPageSession>()
            visibleReports += Triple(session.id, secondArg(), session.open)
        }
        every { telemetry.report(capture(reportedSession)) } just Runs
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
        const val CREATED_AT_MS = 77L
        const val NOW_MS = 10_000L
        const val PRODUCT_URL = "https://product.paseo/app"
        val PRODUCT_ID = ProductId.parse("product.paseo").getOrThrow()
    }
}
