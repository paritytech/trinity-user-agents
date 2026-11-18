package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import android.content.Context
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

private const val PRODUCT = "footer-test.dot"

// Posts ready, tries a request off the product origin, presses once and echoes what the host tells it.
private const val PAGE = """<!doctype html><html><body style="background:transparent"><script>
function post(m) { window.ProductFooter.postMessage(JSON.stringify(m)); }
window.productFooter = { receive: function (m) { post({ kind: 'action', actionId: 'got-' + m.kind + '-' + (m.state || '') }); } };
post({ kind: 'ready', height: 96 });
fetch('https://example.com/').then(function (r) { post({ kind: 'action', actionId: r.ok ? 'offsite-loaded' : 'offsite-refused' }); },
  function () { post({ kind: 'action', actionId: 'offsite-refused' }); });
post({ kind: 'action', actionId: 'go', payload: { n: 1 } });
</script></body></html>"""

@RunWith(AndroidJUnit4::class)
class ProductRoomFooterWebLayoutTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context: Context = instrumentation.targetContext
    private val archive = File(context.cacheDir, "footer-test-archive")
    private val actions = CopyOnWriteArrayList<ProductFooterMessage.Action>()
    private val heights = CopyOnWriteArrayList<Int>()
    private lateinit var layout: ProductRoomFooterWebLayout

    @Before
    fun setUp() {
        archive.mkdirs()
        File(archive, "footer.html").writeText(PAGE)
    }

    @After
    fun tearDown() {
        instrumentation.runOnMainSync { layout.release() }
        archive.deleteRecursively()
    }

    @Test
    fun thePageOfTheProductReportsAndHearsBackAndCannotReachOtherOrigins() {
        val pressed = CountDownLatch(1)
        val echoed = CountDownLatch(1)
        val offsite = CountDownLatch(1)
        open { action ->
            when {
                action.actionId == "go" -> pressed.countDown()
                action.actionId.startsWith("got-") -> echoed.countDown()
                action.actionId.startsWith("offsite-") -> offsite.countDown()
            }
        }

        assertTrue("the page's press never arrived", pressed.await(20, TimeUnit.SECONDS))
        instrumentation.runOnMainSync { layout.send(ProductFooterHostEvent.Confirmed) }
        assertTrue("the page never heard the host", echoed.await(10, TimeUnit.SECONDS))
        assertTrue("the off-origin request never settled", offsite.await(10, TimeUnit.SECONDS))

        assertEquals(listOf(96), heights)
        assertEquals(ProductFooterMessage.Action("go", """{"n":1}"""), actions.first { it.actionId == "go" })
        assertTrue(actions.any { it.actionId == "got-state-confirmed" })
        assertTrue(actions.any { it.actionId == "offsite-refused" })
    }

    private fun open(onAction: (ProductFooterMessage.Action) -> Unit) {
        val resolver = mockk<DotNsResolver> {
            coEvery { resolveToLocalUri(any()) } returns Result.success(Uri.fromFile(archive))
        }
        val tld = mockk<DotNsTldProvider> {
            coEvery { getTld() } returns Result.success(DotNsTld.parse("dot")!!)
            every { currentTldOrNull() } returns DotNsTld.parse("dot")
        }
        val url = requireNotNull(ProductRoomFooterWebPage.url("footer.html", PRODUCT))
        instrumentation.runOnMainSync {
            layout = ProductRoomFooterWebLayout(
                context = context,
                productId = PRODUCT,
                pageUrl = url,
                content = DotNsWebViewClient(resolver, tld),
                onAction = { actions += it; onAction(it) },
                onHeight = { heights += it },
            )
        }
    }
}
