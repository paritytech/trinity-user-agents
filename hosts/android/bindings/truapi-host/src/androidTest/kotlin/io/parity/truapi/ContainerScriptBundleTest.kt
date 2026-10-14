package io.parity.truapi

import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class ContainerScriptBundleTest {
    @Test
    fun endpointStaysPrivateAndChildFramesCannotBypassContainer() = verifyContainer(nativeHttp = false)

    @Test
    fun nativeHttpLeavesFetchToHostWhileWebSocketsAndMediaStayProtected() = verifyContainer(nativeHttp = true)

    private fun verifyContainer(nativeHttp: Boolean) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val container = ContainerScriptBundle.load(context)
        val reports = FrameReports()
        lateinit var webView: WebView

        instrumentation.runOnMainSync {
            check(WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT))
            webView = WebView(context)
            webView.settings.javaScriptEnabled = true
            webView.addJavascriptInterface(reports, "frameReport")
            webView.webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                    val frames = if (request.isForMainFrame) {
                        "<iframe src='/child'></iframe><iframe src='https://other.test/child'></iframe>"
                    } else ""
                    val html = """
                        <!doctype html><html><body>$frames<script>
                        const host = window.__HOST_API_CLIENT__;
                        window.__HOST_API_CLIENT__ = {};
                        const originalFetch = window.fetch;
                        const originalWebSocket = window.WebSocket;
                        const originalGetUserMedia = navigator.mediaDevices.getUserMedia;
                        window.fetch = () => {};
                        window.WebSocket = () => {};
                        navigator.mediaDevices.getUserMedia = () => {};
                        frameReport.receive(
                          location.href,
                          host !== undefined && window.__HOST_API_CLIENT__ === host && Object.isFrozen(host),
                          typeof Object.getOwnPropertyDescriptor(window, '__HOST_API_PORT__')?.get === 'function',
                          window.__truapi_localhost === undefined,
                          window.__HOST_WEBVIEW_MARK__ === true,
                          window.fetch === originalFetch,
                          window.WebSocket === originalWebSocket,
                          navigator.mediaDevices.getUserMedia === originalGetUserMedia
                        );
                        </script></body></html>
                    """.trimIndent()
                    return WebResourceResponse("text/html", "UTF-8", html.byteInputStream())
                }
            }
            WebViewCompat.addDocumentStartJavaScript(
                webView,
                "if (window === window.top) window.__truapi_localhost = {url: 'ws://127.0.0.1:1/'};",
                setOf("https://product.test"),
            )
            WebViewCompat.addDocumentStartJavaScript(
                webView,
                "window.__truapi_localhost = {...window.__truapi_localhost, nativeHttp: $nativeHttp};\n$container",
                setOf("*"),
            )
            webView.loadUrl("https://product.test/")
        }

        try {
            val complete = reports.finished.await(30, TimeUnit.SECONDS)
            assertTrue("missing frame reports: ${reports.values}", complete)
            assertEquals(
                mapOf(
                    "https://product.test/" to FrameState(true, true, true, true, !nativeHttp, true, true),
                    "https://product.test/child" to FrameState(false, false, true, false, !nativeHttp, true, true),
                    "https://other.test/child" to FrameState(false, false, true, false, !nativeHttp, true, true),
                ),
                reports.values,
            )
        } finally {
            instrumentation.runOnMainSync { webView.destroy() }
        }
    }

    private data class FrameState(
        val hasClient: Boolean,
        val hasLegacyPort: Boolean,
        val endpointPrivate: Boolean,
        val legacyHostedFlag: Boolean,
        val fetchLocked: Boolean,
        val webSocketLocked: Boolean,
        val mediaLocked: Boolean,
    )

    private class FrameReports {
        val finished = CountDownLatch(3)
        val values = ConcurrentHashMap<String, FrameState>()

        @JavascriptInterface
        fun receive(
            url: String,
            hasClient: Boolean,
            hasLegacyPort: Boolean,
            endpointPrivate: Boolean,
            legacyHostedFlag: Boolean,
            fetchLocked: Boolean,
            webSocketLocked: Boolean,
            mediaLocked: Boolean,
        ) {
            values[url] = FrameState(hasClient, hasLegacyPort, endpointPrivate, legacyHostedFlag, fetchLocked, webSocketLocked, mediaLocked)
            finished.countDown()
        }
    }
}
