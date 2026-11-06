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
import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenMarks
import io.paritytech.polkadotapp.common.data.telemetry.HostSessionIds
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.PageReveal
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.job
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
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
    private val events = mutableListOf<HostEvent>()
    private val capture = HostEventCapture { events += it() }
    private val clock = HostMonotonicTimeProvider { OPEN_REQUESTED_AT }
    private var minted = 0
    private val sessionIds = HostSessionIds { "session-${++minted}" }
    private val cacheHits = HostCacheHits()
    private val taps = HostOpenMarks(clock, { TAPPED_AT_EPOCH }, capture, sessionIds, cacheHits)
    private val installedOrigins = mutableListOf<HostPageOrigin?>()
    private val pageOpenTelemetry = HostPageOpenTelemetry(
        originProvider = { HostEventOrigin("nightly", "gp", "1.2.3", "android", "15", 123L, "launch-7") },
        timeProvider = clock,
        capture = capture,
        marks = taps,
        sessionIds = sessionIds,
        cacheHits = cacheHits,
    )
    private val hostLoads = mutableListOf<HostLoadContext>()
    private val revealedListeners = mutableListOf<(PageReveal) -> Unit>()
    private val attachedSessionIds = mutableListOf<String?>()
    private val pageFinishedListeners = mutableListOf<() -> Unit>()
    private val mainFrameFailedListeners = mutableListOf<() -> Unit>()
    private val pageStartedListeners = mutableListOf<(String) -> Unit>()

    private val starter = TrUAPISessionStarter(
        hostBridgeFactory = hostBridgeFactory,
        runtimeProvider = runtimeProvider,
        chainDirectory = chainDirectory,
        dotNsTldProvider = dotNsTldProvider,
        bootstrapInstaller = bootstrapInstaller,
        visibleProducts = visibleProducts,
        hostWebViewCreatedTelemetry = telemetry,
        hostPageOpenTelemetry = pageOpenTelemetry,
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

    // The page load is joined to the SPA's own spans by this id, so it has to be the one the bootstrap hands
    // the product, and a page that finishes again (a reload, a replaced WebView) is not a second load.
    @Test
    fun `the first finished page reports the load under the session id exposed to the product`() = runBlocking<Unit> {
        starter.start(
            provider = provider,
            productUrl = PRODUCT_URL,
            scope = this,
            hostApiNavigation = navigation,
            explicitProductId = PRODUCT_ID,
        )
        yield()

        pageFinishedListeners.forEach { it() }
        pageFinishedListeners.forEach { it() }

        assertEquals(listOf("ok" to installerSessionId.captured), pageLoads())
    }

    @Test
    fun `a page whose main document failed is reported as a failed load`() = runBlocking<Unit> {
        starter.start(
            provider = provider,
            productUrl = PRODUCT_URL,
            scope = this,
            hostApiNavigation = navigation,
            explicitProductId = PRODUCT_ID,
        )
        yield()

        mainFrameFailedListeners.forEach { it() }
        pageFinishedListeners.forEach { it() }

        assertEquals(listOf("error" to installerSessionId.captured), pageLoads())
    }

    // A navigation that replaces a failed one starts a new document, and that document is what loads.
    @Test
    fun `a failure on a document that a later navigation replaced does not mark the load as failed`() =
        runBlocking<Unit> {
            starter.start(
                provider = provider,
                productUrl = PRODUCT_URL,
                scope = this,
                hostApiNavigation = navigation,
                explicitProductId = PRODUCT_ID,
            )
            yield()

            pageStartedListeners.forEach { it(PRODUCT_URL) }
            mainFrameFailedListeners.forEach { it() }
            pageStartedListeners.forEach { it(PRODUCT_URL) }
            pageFinishedListeners.forEach { it() }

            assertEquals(listOf("ok" to installerSessionId.captured), pageLoads())
        }

    // The archive lookups behind the page's requests and its chain connects report under this id, so
    // they join the page's own spans.
    @Test
    fun `the page's archive lookups and chain connects carry the session id exposed to the product`() =
        runBlocking<Unit> {
            startPage(this)

            assertEquals(listOf(installerSessionId.captured), hostLoads.map { it.sessionId })
            assertEquals(listOf<String?>(installerSessionId.captured), attachedSessionIds)
        }

    // Two pages of one product each keep their own session: closing one must not take the other's.
    @Test
    fun `a second page of the same product keeps its own session after the first closes`() = runBlocking<Unit> {
        val firstScope = CoroutineScope(coroutineContext + Job())
        startPage(firstScope)
        val firstSession = installerSessionId.captured
        startPage(this)
        val secondSession = installerSessionId.captured

        firstScope.coroutineContext.job.cancelAndJoin()
        pageFinishedListeners.last()()

        assertNotEquals(firstSession, secondSession)
        assertEquals(listOf("abandoned" to firstSession, "ok" to secondSession), pageLoads())
    }

    @Test
    fun `a page closed before it finished loading is reported as abandoned`() = runBlocking<Unit> {
        val productScope = CoroutineScope(coroutineContext + Job())
        startPage(productScope)

        productScope.coroutineContext.job.cancelAndJoin()
        pageFinishedListeners.forEach { it() }

        assertEquals(listOf("abandoned" to installerSessionId.captured), pageLoads())
    }

    // Without an event the open would be missing from the data instead of counted as failed.
    @Test
    fun `an open whose runtime does not boot is reported as an error`() = runBlocking<Unit> {
        coEvery { runtimeProvider.runtime() } returns Result.failure(IllegalStateException("no suffix"))

        startPage(this)

        val (outcome, sessionId) = pageLoads().single()
        assertEquals("error", outcome)
        assertNotNull(sessionId)
    }

    @Test
    fun `the page's reveal is reported under its session`() = runBlocking<Unit> {
        startPage(this)

        revealedListeners.forEach { it(PageReveal.THEME) }

        val revealed = events.single { it.name == "host.webview.revealed" }
        assertEquals("theme" to installerSessionId.captured, revealed.tags["reveal.via"] to revealed.tags["host.session_id"])
    }

    // The page times its own load from the tap, so it gets the tap's wall-clock time; nothing else does.
    @Test
    fun `only the page a card tap opened is handed the tap time`() = runBlocking<Unit> {
        taps.tapped(PRODUCT_ID.value)
        startPage(this)
        startPage(this)

        assertEquals(listOf(TAPPED_AT_EPOCH, null), installedOrigins.map { it?.openedAtEpochMs })
        assertEquals("pocket_card", installedOrigins.first()?.openSource)
    }

    // The tap's own fetch ran under the session it minted; the page takes it, so its lookups join that
    // fetch instead of reporting the wait on it as a cache hit of their own.
    @Test
    fun `a page a card tap opened runs under the session the tap minted`() = runBlocking<Unit> {
        taps.tapped(PRODUCT_ID.value)
        val tapSession = taps.pendingSession(PRODUCT_ID.value)

        startPage(this)

        assertEquals("session-1", tapSession)
        assertEquals(listOf(tapSession, tapSession), listOf(installerSessionId.captured, hostLoads.single().sessionId))
        val tapped = events.single { it.name == "host.pocket.card_tapped" }
        assertEquals("opened" to tapSession, tapped.tags["outcome"] to tapped.tags["host.session_id"])
    }

    private suspend fun startPage(scope: CoroutineScope) {
        starter.start(
            provider = provider,
            productUrl = PRODUCT_URL,
            scope = scope,
            hostApiNavigation = navigation,
            explicitProductId = PRODUCT_ID,
        )
        yield()
    }

    private fun pageLoads() = events.filter { it.name == "host.webview.page_loaded" }
        .map { it.tags["outcome"] to it.tags["host.session_id"] }

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
        every { bootstrapInstaller.installerFor(any(), capture(installerSessionId), any()) } answers {
            installedOrigins += thirdArg<HostPageOrigin?>()
            val installer: (String) -> (WebView) -> Unit = { { } }
            installer
        }
        coEvery { provider.addWebViewSetup(capture(webViewSetups)) } just Runs
        coEvery { provider.loadInitialContent() } just Runs
        every { provider.useTrUAPIPermissions(any(), any()) } just Runs
        every { telemetry.report(capture(reportedSessionId)) } just Runs
        every { provider.useHostLoad(capture(hostLoads)) } just Runs
        every { provider.addOnRevealedListener(capture(revealedListeners)) } just Runs
        every { provider.addOnPageFinishedListener(capture(pageFinishedListeners)) } just Runs
        every { provider.addOnMainFrameFailedListener(capture(mainFrameFailedListeners)) } just Runs
        every { provider.addOnPageStartedListener(capture(pageStartedListeners)) } just Runs
        coEvery { bridge.attach(any(), any(), any(), any(), any(), any(), any(), any(), any()) } coAnswers {
            attachedSessionIds += arg<String?>(7)
            arg<suspend (String) -> Unit>(8)("endpoint-bootstrap")
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
        const val OPEN_REQUESTED_AT = 1_000L
        const val TAPPED_AT_EPOCH = 1_793_880_000_000L
        val PRODUCT_ID = ProductId.parse("product.paseo").getOrThrow()
    }
}
