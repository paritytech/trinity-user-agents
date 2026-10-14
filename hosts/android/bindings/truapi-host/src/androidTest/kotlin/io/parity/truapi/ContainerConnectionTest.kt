package io.parity.truapi

import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.webkit.WebViewCompat
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import javax.net.ServerSocketFactory

@RunWith(AndroidJUnit4::class)
class ContainerConnectionTest {
    @Test
    fun productAndPermissionsShareSocketAndRecoverAfterAbruptLoss() = verifyConnection(legacy = false)

    @Test
    fun legacyMessagePortSupportsStartupHandshake() = verifyConnection(legacy = true)

    private fun verifyConnection(legacy: Boolean) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val container = ContainerScriptBundle.load(context)
        val reports = Reports()
        val connections = AtomicInteger()
        val permissionChecks = AtomicInteger()
        val sockets = SocketFactory()
        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                connections.incrementAndGet()
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                val frame = bytes.toByteArray()
                val requestIdSize = (frame[0].toInt() and 0xff) / 4 + 1
                when (frame[requestIdSize].toInt() to frame[requestIdSize + 1].toInt()) {
                    1 to 0 -> webSocket.send(response(frame, requestIdSize, permissionDenied = false))
                    10 to 2 -> {
                        permissionChecks.incrementAndGet()
                        webSocket.send(response(frame, requestIdSize, permissionDenied = true))
                    }
                    else -> error("Unexpected wire address")
                }
            }
        }

        MockWebServer().use { server ->
            server.serverSocketFactory = sockets
            repeat(2) { server.enqueue(MockResponse.Builder().webSocketUpgrade(listener).build()) }
            server.start()
            lateinit var webView: WebView
            instrumentation.runOnMainSync {
                webView = WebView(context)
                webView.settings.javaScriptEnabled = true
                webView.addJavascriptInterface(reports, "connectionReport")
                webView.webViewClient = object : WebViewClient() {
                    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                        val html = """
                            <!doctype html><html><script>
                            ${if (legacy) LEGACY_CLIENT else SHARED_CLIENT}
                            sendCall();
                            window.checkPermission = () => {
                              const deniedSocket = new WebSocket('wss://denied.test/');
                              deniedSocket.onerror = () => connectionReport.permissionDenied();
                            };
                            checkPermission();
                            </script></html>
                        """.trimIndent()
                        return WebResourceResponse("text/html", "UTF-8", html.byteInputStream())
                    }
                }
                WebViewCompat.addDocumentStartJavaScript(
                    webView,
                    "window.__truapi_localhost = {url: 'ws://localhost:${server.port}/', nativeHttp: true};",
                    setOf("http://localhost"),
                )
                WebViewCompat.addDocumentStartJavaScript(webView, container, setOf("*"))
                webView.loadUrl("http://localhost/")
            }

            try {
                assertEquals("true", reports.responses.poll(15, TimeUnit.SECONDS))
                assertEquals(true, reports.denied.poll(15, TimeUnit.SECONDS))
                assertEquals(listOf(1, 1), listOf(connections.get(), permissionChecks.get()))

                if (!legacy) {
                    sockets.accepted.remove().apply {
                        setSoLinger(true, 0)
                        close()
                    }
                    assertTrue("shared SDK client did not signal loss", reports.lost.await(15, TimeUnit.SECONDS))
                    instrumentation.runOnMainSync { webView.evaluateJavascript("sendCall(); checkPermission()", null) }

                    assertEquals("true", reports.responses.poll(15, TimeUnit.SECONDS))
                    assertEquals(true, reports.denied.poll(15, TimeUnit.SECONDS))
                    assertEquals(listOf(2, 2), listOf(connections.get(), permissionChecks.get()))
                }
            } finally {
                instrumentation.runOnMainSync { webView.destroy() }
            }
        }
    }

    private class SocketFactory : ServerSocketFactory() {
        val accepted = LinkedBlockingQueue<Socket>()

        override fun createServerSocket(): ServerSocket = object : ServerSocket() {
            override fun accept(): Socket = super.accept().also { accepted.add(it) }
        }

        override fun createServerSocket(port: Int): ServerSocket =
            createServerSocket().apply { bind(InetSocketAddress(port)) }

        override fun createServerSocket(port: Int, backlog: Int): ServerSocket =
            createServerSocket().apply { bind(InetSocketAddress(port), backlog) }

        override fun createServerSocket(port: Int, backlog: Int, address: InetAddress): ServerSocket =
            createServerSocket().apply { bind(InetSocketAddress(address, port), backlog) }
    }

    private fun response(request: ByteArray, requestIdSize: Int, permissionDenied: Boolean): ByteString {
        val response = request.copyOf(requestIdSize + if (permissionDenied) 6 else 5)
        response[requestIdSize + 2] = 1
        response.fill(0, requestIdSize + 3)
        return response.toByteString()
    }

    private class Reports {
        val responses = LinkedBlockingQueue<String>()
        val lost = CountDownLatch(1)
        val denied = LinkedBlockingQueue<Boolean>()

        @JavascriptInterface
        fun response(value: String) {
            responses.add(value)
        }

        @JavascriptInterface
        fun disconnected() {
            lost.countDown()
        }

        @JavascriptInterface
        fun permissionDenied() {
            denied.add(true)
        }
    }

    private companion object {
        val SHARED_CLIENT = """
            const host = window.__HOST_API_CLIENT__;
            const client = host.client;
            let connected = false;
            host.subscribeConnectionStatus(status => {
              if (status === 'connected') connected = true;
              if (connected && status === 'disconnected') connectionReport.disconnected();
            });
            window.sendCall = async () => {
              const result = await client.system.handshake();
              connectionReport.response(String(result.isOk() && host.client === client));
            };
        """.trimIndent()

        val LEGACY_CLIENT = """
            const port = window.__HOST_API_PORT__;
            window.sendCall = () => {
              port.onmessage = event => connectionReport.response(String(
                JSON.stringify(Array.from(event.data)) === '[16,112,105,110,103,1,0,1,0,0]'
              ));
              port.postMessage(new Uint8Array([16,112,105,110,103,1,0,0,0,3]));
            };
        """.trimIndent()
    }
}
