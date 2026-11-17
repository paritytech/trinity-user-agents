package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import android.os.SystemClock
import android.view.Gravity
import android.view.MotionEvent
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductFooterHostEvent
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductRoomFooterWebLayout
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductRoomFooterWebPage
import org.junit.After
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

private const val PRODUCT = "overlay-touch-test.dot"

// Asks for an overlay whenever the host tells it something, then reports that it asked with an action.
private const val FOOTER_PAGE = """<!doctype html><html><body><script>
var post = function (message) { window.ProductFooter.postMessage(JSON.stringify(message)); };
window.productFooter = { receive: function () {
  post({ kind: 'sendOverlay', path: 'overlay.html' });
  post({ kind: 'action', actionId: 'asked' });
} };
post({ kind: 'ready', height: 64 });
</script></body></html>"""

// Never says it is done.
private const val OVERLAY_PAGE = """<!doctype html><html><body style="background:transparent"></body></html>"""

/** The footer honours `sendOverlay` only just after a touch that ended on it (ProductOverlayTouchRule). */
@RunWith(AndroidJUnit4::class)
class ProductRoomFooterOverlayTouchTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val archive = File(instrumentation.targetContext.cacheDir, "overlay-touch-test-archive")
    private lateinit var scenario: ActivityScenario<ComponentActivity>
    private lateinit var screen: ViewGroup
    private lateinit var footer: ProductRoomFooterWebLayout
    @Volatile private var revealed = false
    @Volatile private var asked = false

    @Before
    fun setUp() {
        archive.mkdirs()
        File(archive, "footer.html").writeText(FOOTER_PAGE)
        File(archive, "overlay.html").writeText(OVERLAY_PAGE)
        scenario = ActivityScenario.launch(ComponentActivity::class.java)
        scenario.onActivity { activity ->
            val root = FrameLayout(activity)
            activity.setContentView(root)
            // The overlay goes over the whole screen, the activity's content view, next to the root.
            screen = activity.findViewById(android.R.id.content)
            footer = ProductRoomFooterWebLayout(
                context = activity,
                productId = PRODUCT,
                pageUrl = requireNotNull(ProductRoomFooterWebPage.url("footer.html", PRODUCT)),
                content = archiveContent(archive),
                onAction = { if (it.actionId == "asked") asked = true },
                onHeight = {},
                onRevealed = { revealed = true }
            )
            root.addView(footer, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 200, Gravity.BOTTOM))
        }
        assertTrue("the footer page never loaded", waitUntil(10_000) { revealed })
    }

    @After
    fun tearDown() {
        instrumentation.runOnMainSync { shownOverlay()?.dismiss() }
        scenario.close()
        archive.deleteRecursively()
    }

    @Test
    fun theFooterShowsAnOverlayJustAfterATouchOnIt() {
        touch(endY = 100f)
        ask()

        assertTrue("the footer's overlay never showed", waitUntil(5_000) { shownOverlay() != null })
    }

    @Test
    fun theFooterDropsAnOverlayAskedForWithNoTouch() {
        ask()

        assertNull(overlay())
    }

    @Test
    fun theFooterDropsAnOverlayAskedForAfterTheWindow() {
        touch(endY = 100f)
        Thread.sleep(ProductOverlayTouchRule.WINDOW_MILLIS + 200)
        ask()

        assertNull(overlay())
    }

    @Test
    fun aTouchThatEndedOutsideTheFooterDoesNotCount() {
        // The finger went down on the footer and lifted above it, over the chat.
        touch(endY = -40f)
        ask()

        assertNull(overlay())
    }

    /** A finger down in the middle of the footer and up at [endY], through the footer as the system would deliver it. */
    private fun touch(endY: Float) {
        instrumentation.runOnMainSync {
            val x = footer.width / 2f
            val now = SystemClock.uptimeMillis()
            listOf(MotionEvent.ACTION_DOWN to 100f, MotionEvent.ACTION_UP to endY).forEach { (action, y) ->
                val event = MotionEvent.obtain(now, SystemClock.uptimeMillis(), action, x, y, 0)
                footer.dispatchTouchEvent(event)
                event.recycle()
            }
        }
    }

    /** Has the host tell the page something, which makes it ask for an overlay, and waits until it has asked. */
    private fun ask() {
        asked = false
        instrumentation.runOnMainSync { footer.send(ProductFooterHostEvent.Reset) }
        assertTrue("the page never asked", waitUntil(5_000) { asked })
        // The action follows the overlay request, so the request has been handled by now.
        instrumentation.waitForIdleSync()
    }

    /** The overlay over the screen, looked up from the test thread. */
    private fun overlay(): ProductOverlayWebLayout? {
        var found: ProductOverlayWebLayout? = null
        instrumentation.runOnMainSync { found = shownOverlay() }
        return found
    }

    /** The overlay over the screen; on the main thread only. */
    private fun shownOverlay(): ProductOverlayWebLayout? =
        (0 until screen.childCount).map(screen::getChildAt).filterIsInstance<ProductOverlayWebLayout>().firstOrNull()

    private fun waitUntil(timeoutMillis: Long, condition: () -> Boolean): Boolean {
        val deadline = SystemClock.uptimeMillis() + timeoutMillis
        while (SystemClock.uptimeMillis() < deadline) {
            var met = false
            instrumentation.runOnMainSync { met = condition() }
            if (met) return true
            Thread.sleep(50)
        }
        return false
    }
}
