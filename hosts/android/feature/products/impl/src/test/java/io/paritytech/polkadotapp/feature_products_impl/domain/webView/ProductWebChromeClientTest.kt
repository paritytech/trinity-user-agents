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

    /** Console output keeps its priority and carries no tag, so none of it reaches Sentry. */
    @Test
    fun `console lines keep their own level and carry no tag`() {
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.LOG, "rendered"))
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.WARNING, "slow follow"))
        client.onConsoleMessage(consoleMessage(ConsoleMessage.MessageLevel.ERROR, "No answer within 45000 ms"))

        verify(exactly = 1) {
            tree.log(Log.DEBUG, tag = null, "Script product.paseo: rendered [app.js:7]", null)
            tree.log(Log.WARN, tag = null, "Script product.paseo: slow follow [app.js:7]", null)
            tree.log(Log.ERROR, tag = null, "Script product.paseo: No answer within 45000 ms [app.js:7]", null)
        }
        verify(exactly = 0) { tree.log(any(), ofType<String>(), any(), any()) }
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
