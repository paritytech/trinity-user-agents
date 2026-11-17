package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductRoomFooterWebPage
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

private const val PRODUCT = "overlay-test.dot"

// Says it is done a moment after it loads.
private const val DONE_PAGE = """<!doctype html><html><body style="background:transparent"><script>
setTimeout(function () { window.ProductOverlay.postMessage(JSON.stringify({ kind: 'overlayDone' })); }, 200);
</script></body></html>"""

// Never says it is done.
private const val STUCK_PAGE = """<!doctype html><html><body style="background:transparent"></body></html>"""

@RunWith(AndroidJUnit4::class)
class ProductOverlayWebLayoutTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val archive = File(instrumentation.targetContext.cacheDir, "overlay-test-archive")
    private lateinit var scenario: ActivityScenario<ComponentActivity>
    private lateinit var underneath: View
    private var touchesUnderneath = 0

    @Before
    fun setUp() {
        archive.mkdirs()
        File(archive, "done.html").writeText(DONE_PAGE)
        File(archive, "stuck.html").writeText(STUCK_PAGE)
        scenario = ActivityScenario.launch(ComponentActivity::class.java)
        scenario.onActivity { activity ->
            underneath = View(activity).apply {
                setOnTouchListener { _, _ -> touchesUnderneath++; true }
            }
            activity.setContentView(underneath)
        }
    }

    @After
    fun tearDown() {
        scenario.close()
        archive.deleteRecursively()
    }

    @Test
    fun theOverlayCoversTheScreenTakesNoTouchAndGoesWhenItsPageIsDone() {
        // A limit well past the wait below, so only the page saying it is done can end the overlay in time.
        val overlay = show("done.html", limitMillis = 60_000)
        assertNotNull(overlay)

        instrumentation.runOnMainSync {
            assertTrue(overlay!!.isShowing)
            val screen = overlay.parent as ViewGroup
            assertEquals(screen.width, overlay.width)
            assertEquals(screen.height, overlay.height)
            assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS, overlay.importantForAccessibility)
            tap(screen)
        }
        assertEquals("the tap never reached the view under the overlay", 2, touchesUnderneath)

        assertTrue("the overlay stayed after its page was done", waitUntil(10_000) { !overlay!!.isShowing })
    }

    @Test
    fun onlyOneOverlayShowsAtATime() {
        val first = show("stuck.html")
        val second = show("done.html")

        assertNotNull(first)
        assertNull(second)
        instrumentation.runOnMainSync { first!!.dismiss() }
        assertNotNull("a new overlay was refused after the first went", show("done.html"))
    }

    @Test
    fun anOverlayWhosePageNeverEndsGoesAtItsLimit() {
        val overlay = show("stuck.html", limitMillis = 500)
        assertNotNull(overlay)

        assertTrue("the overlay outlived its limit", waitUntil(5_000) { !overlay!!.isShowing })
        instrumentation.runOnMainSync { assertFalse(overlay!!.isShowing) }
    }

    @Test
    fun anOverlayIsFreedWhenItsScreenGoesAway() {
        val overlay = show("stuck.html")
        assertNotNull(overlay)

        // The activity is destroyed and made again while the overlay shows; nothing calls dismiss().
        scenario.recreate()
        instrumentation.waitForIdleSync()

        instrumentation.runOnMainSync {
            assertTrue("the overlay's webview outlived its screen", overlay!!.isReleased)
            assertFalse(overlay.isShowing)
        }
    }

    private fun show(page: String, limitMillis: Long = ProductOverlayWebLayout.OVERLAY_LIMIT_MILLIS): ProductOverlayWebLayout? {
        val url = requireNotNull(ProductRoomFooterWebPage.url(page, PRODUCT))
        var overlay: ProductOverlayWebLayout? = null
        instrumentation.runOnMainSync {
            overlay = ProductOverlayWebLayout.show(underneath, PRODUCT, url, archiveContent(archive), limitMillis)
        }
        instrumentation.waitForIdleSync()
        return overlay
    }

    /** A finger down and up in the middle of [screen], through the screen as the system would deliver it. */
    private fun tap(screen: ViewGroup) {
        val x = screen.width / 2f
        val y = screen.height / 2f
        val now = SystemClock.uptimeMillis()
        listOf(MotionEvent.ACTION_DOWN, MotionEvent.ACTION_UP).forEach { action ->
            val event = MotionEvent.obtain(now, now, action, x, y, 0)
            screen.dispatchTouchEvent(event)
            event.recycle()
        }
    }

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
