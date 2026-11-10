package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.util.Log
import android.webkit.ConsoleMessage
import io.mockk.every
import io.mockk.mockk
import io.mockk.spyk
import io.mockk.verify
import org.junit.After
import org.junit.Before
import org.junit.Test
import timber.log.Timber

class ProductWebChromeClientTest {
    private val tree = spyk(
        object : Timber.Tree() {
            public override fun log(priority: Int, tag: String?, message: String, t: Throwable?) = Unit
        },
    )
    private val client = chromeClient(logPrefix = "Script product.paseo")

    @Before
    fun plant() {
        Timber.plant(tree)
    }

    @After
    fun uproot() {
        Timber.uprootAll()
    }

    /** Tagged lines reach Sentry, so product console warnings and errors leave the device and the rest stays on it. */
    @Test
    fun `only console warnings and errors are tagged for Sentry`() {
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.LOG, "rendered"))
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.WARNING, "slow follow"))
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.ERROR, "No answer within 45000 ms"))

        verify(exactly = 1) {
            tree.log(Log.DEBUG, tag = null, "Script product.paseo: rendered [app.js:7]", null)
            tree.log(Log.WARN, "truapi.product", "slow follow", null)
            tree.log(Log.ERROR, "truapi.product", "No answer within 45000 ms", null)
        }
    }

    /** A console line can carry a whole payload, and only the first 512 characters leave the device. */
    @Test
    fun `sent lines are capped at 512 characters`() {
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.ERROR, "x".repeat(600)))

        verify(exactly = 1) {
            tree.log(Log.ERROR, "truapi.product", "x".repeat(512), null)
        }
    }

    /** A browser tab prefixes its lines with the page URL, which can carry a token, so the sent line is the message alone. */
    @Test
    fun `sent lines leave out the page and script URLs`() {
        val browserClient = chromeClient(logPrefix = "Browser: https://site.example/?token=abc")

        browserClient.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.ERROR, "boom"))

        verify(exactly = 1) {
            tree.log(Log.ERROR, "truapi.product", "boom", null)
        }
    }

    private fun chromeClient(logPrefix: String) = ProductWebChromeClient(
        permissionGuard = mockk(),
        devicePermissionHandler = mockk(),
        contextManager = mockk(),
        fileProvider = mockk(),
        logPrefix = logPrefix,
        callingProductIdProvider = mockk(),
        scope = mockk(),
        onTitleReceived = null,
    )

    private fun consoleMessage(level: ConsoleMessage.MessageLevel, text: String): ConsoleMessage = mockk {
        every { messageLevel() } returns level
        every { message() } returns text
        every { sourceId() } returns "app.js"
        every { lineNumber() } returns 7
    }
}
