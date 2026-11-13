package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenSource
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpen
import io.paritytech.polkadotapp.common.data.telemetry.HostPageOpens
import io.paritytech.polkadotapp.common.data.telemetry.HostPageSession
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.toUrl
import io.paritytech.polkadotapp.feature_products_impl.data.repository.BrowserTabRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class RealProductSessionControllerTest {
    private val product = ProductId.parse("loyalty.dot").getOrThrow()
    private val link = HostPageOpen(HostOpenSource.DEEPLINK, epochMs = 1_000L, monotonicMs = 1_000L, sessionId = "link-session")

    private val origin = HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L)
    private val events = mutableListOf<HostEvent>()
    private val pageOpens = HostPageOpens({ origin }, { events += it() }, { 2_000L })

    // Stands in for the TrUAPI tab factory, whose session starter starts the product's host session in the tab's scope.
    private val sessions = mutableListOf<HostPageSession>()
    private val tabSessionFactory = mockk<ProductTabSessionFactory> {
        every { create(any(), any(), any()) } answers {
            secondArg<CoroutineScope>().launch { sessions += pageOpens.startSession(product.value) }
            mockk<BrowserWebViewProvider>(relaxed = true) {
                every { webViews() } returns emptyFlow()
                every { loadProgress } returns emptyFlow()
            }
        }
    }

    private val dispatcher = UnconfinedTestDispatcher()
    private val dot = DotNsTld.parse("dot")!!

    private lateinit var controller: RealProductSessionController

    @Before
    fun setUp() {
        mockkStatic(Uri::class)
        every { Uri.parse(any()) } answers {
            val text = firstArg<String>()
            mockk<Uri> { every { this@mockk.toString() } returns text }
        }
        controller = RealProductSessionController(
            tabSessionFactory = tabSessionFactory,
            productRegistrar = mockk(relaxed = true),
            deepLinkHandler = mockk(relaxed = true),
            appNotifier = mockk(relaxed = true),
            repository = mockk<BrowserTabRepository>(relaxed = true) { coEvery { loadAll() } returns emptyList() },
            productRepository = mockk<ProductRepository> { every { observeProducts() } returns flowOf(emptyList()) },
            ipfsContentLookup = mockk(relaxed = true),
            context = mockk(relaxed = true),
            dispatchers = mockk<CoroutineDispatchers> {
                every { main } returns dispatcher
                every { io } returns dispatcher
                every { computation } returns dispatcher
            },
            dotNsTldProvider = mockk<DotNsTldProvider> {
                every { currentTldOrNull() } returns dot
                coEvery { getTld() } returns Result.success(dot)
            },
            devServers = mockk { every { productAt(any()) } returns null },
            pageOpens = pageOpens,
        )
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
    }

    @Test
    fun `a link into the product's live tab is reported as reused under that tab's session id`() {
        controller.openProduct(product)
        val tab = sessions.single()

        pageOpens.hold(product.value, link)
        controller.openUrl(product.toUrl())

        verify(exactly = 1) { tabSessionFactory.create(any(), any(), any()) }
        val opened = events.single()
        assertEquals("host.pocket.opened", opened.name)
        assertEquals("deeplink", opened.tags["open.source"])
        assertEquals("true", opened.tags["open.reused"])
        assertEquals(tab.id, opened.tags["host.session_id"])
    }

    @Test
    fun `a link with no live tab for its product starts a session that adopts the open`() {
        pageOpens.hold(product.value, link)

        controller.openUrl(product.toUrl())

        assertEquals(HostPageSession(product.value, "link-session", link, isRetry = false), sessions.single())
        val opened = events.single()
        assertEquals("false", opened.tags["open.reused"])
        assertEquals("link-session", opened.tags["host.session_id"])
    }
}
