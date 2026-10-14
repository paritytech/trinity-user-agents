package io.parity.truapi

import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.webkit.WebViewCompat
import kotlinx.coroutines.runBlocking
import mockwebserver3.Dispatcher
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import mockwebserver3.RecordedRequest
import okio.Buffer
import okio.ByteString.Companion.decodeBase64
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.HostFeatureSupportedRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionDecision
import uniffi.truapi.HostRuntimeConfig
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import java.io.Closeable
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.io.path.createTempDirectory

@RunWith(AndroidJUnit4::class)
class ContainerHttpAuthorizationTest {
    @Test
    fun appSharesOneUseHttpGrantsWithRustAndRecovers() = verifyAuthorization(ProductExecutionKind.APP)

    @Test
    fun hiddenWorkerSharesOneUseHttpGrantsWithRustAndRecovers() = verifyAuthorization(ProductExecutionKind.WORKER)

    private fun verifyAuthorization(kind: ProductExecutionKind) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val container = ContainerScriptBundle.load(context)
        val reports = Reports()
        val bridge = PermissionBridge()
        val permission = RemotePermissionRequest(RemotePermission.Remote(listOf("localhost")))
        val hits = LinkedBlockingQueue<String>()
        val authorizedLoads = AtomicInteger()
        val config = HostRuntimeConfig(
            hostName = "HTTP authorization test",
            peopleChainGenesisHash = ByteArray(32),
            bulletinChainGenesisHash = ByteArray(32),
            assetHubChainGenesisHash = ByteArray(32),
            networkSuffix = "paseo",
            databaseDirectory = createTempDirectory("truapi").toString(),
        )

        TrUAPIHostRuntime(bridge, config).use { runtime ->
            runtime.openProductExecution(bridge, ProductExecutionConfig("http-policy.paseo", kind)).use { execution ->
                val endpoint = execution.startWsBridge()
                BridgeProxy(endpoint.port.toInt()).use { proxy ->
                    MockWebServer().use { server ->
                        server.dispatcher = object : Dispatcher() {
                            override fun dispatch(request: RecordedRequest): MockResponse {
                                hits.add(request.url.encodedPath)
                                val image = request.url.encodedPath.startsWith("/image")
                                val body = if (image) Buffer().write(PNG.decodeBase64()!!) else Buffer().writeUtf8("void 0;")
                                return MockResponse.Builder()
                                    .addHeader("Content-Type", if (image) "image/png" else "text/javascript")
                                    .addHeader("Access-Control-Allow-Origin", "*")
                                    .addHeader("Cache-Control", "no-store")
                                    .body(body)
                                    .build()
                            }
                        }
                        server.start()
                        lateinit var webView: WebView
                        instrumentation.runOnMainSync {
                            webView = WebView(context)
                            webView.settings.javaScriptEnabled = true
                            webView.addJavascriptInterface(reports, "httpReport")
                            webView.webViewClient = object : WebViewClient() {
                                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
                                    if (request.isForMainFrame) return WebResourceResponse("text/html", "UTF-8", PAGE.byteInputStream())
                                    if (request.url.host == "product.test") return null
                                    authorizedLoads.incrementAndGet()
                                    val allowed = runBlocking {
                                        execution.authorizeRemotePermission(
                                            RemotePermissionRequest(RemotePermission.Remote(listOf(request.url.host!!))),
                                        )
                                    }
                                    return if (allowed) null else WebResourceResponse(
                                        "text/plain", "UTF-8", 403, "Forbidden",
                                        mapOf("Access-Control-Allow-Origin" to "*"), "denied".byteInputStream(),
                                    )
                                }
                            }
                            val bootstrap = LocalhostBridgeBootstrap.script(proxy.port.toUShort(), endpoint.token)
                            WebViewCompat.addDocumentStartJavaScript(
                                webView,
                                "$bootstrap\nwindow.__truapi_localhost.nativeHttp = true;\n$container",
                                setOf("http://product.test"),
                            )
                            webView.loadUrl("http://product.test/")
                        }

                        fun call(script: String, expected: String) {
                            instrumentation.runOnMainSync {
                                webView.evaluateJavascript(
                                    "Promise.resolve().then(() => $script).then(value => httpReport.result(String(value)))" +
                                        ".catch(error => httpReport.result(String(error)));",
                                    null,
                                )
                            }
                            val result = reports.results.poll(15, TimeUnit.SECONDS)
                            assertEquals("$script (connections=${proxy.connections.get()}, prompts=${bridge.requests.size})", expected, result)
                        }

                        try {
                            assertTrue("product script did not load", reports.ready.await(15, TimeUnit.SECONDS))
                            val operations = listOf("script", "image", "fetch", "xhr")
                            for (operation in operations) {
                                execution.setPermissionAuthorizationStatus(
                                    PermissionAuthorizationRequest.Remote(permission), PermissionAuthorizationStatus.NOT_DETERMINED,
                                )
                                bridge.decisions.add(PermissionDecision.ALLOW_ONCE)
                                call("requestPermission()", "true")
                                call("loadResource('$operation', '${server.url("/$operation-allowed")}')", "true")
                                bridge.decisions.add(PermissionDecision.DENY)
                                call("loadResource('$operation', '${server.url("/$operation-denied")}')", "false")
                            }
                            assertEquals(operations.map { "/$it-allowed" }, hits.toList())
                            assertEquals(List(8) { permission.permission }, bridge.requests.toList())
                            assertEquals(listOf(8, 1), listOf(authorizedLoads.get(), proxy.connections.get()))

                            proxy.disconnect()
                            assertTrue("socket loss did not notify the page", reports.reset.await(15, TimeUnit.SECONDS))
                            execution.setPermissionAuthorizationStatus(
                                PermissionAuthorizationRequest.Remote(permission), PermissionAuthorizationStatus.NOT_DETERMINED,
                            )
                            bridge.decisions.add(PermissionDecision.ALLOW_ONCE)
                            call("requestPermission()", "true")
                            call("loadResource('fetch', '${server.url("/reconnected")}')", "true")
                            assertEquals(operations.map { "/$it-allowed" } + "/reconnected", hits.toList())
                            assertEquals(List(9) { permission.permission }, bridge.requests.toList())
                            assertEquals(listOf(9, 2), listOf(authorizedLoads.get(), proxy.connections.get()))
                        } finally {
                            instrumentation.runOnMainSync { webView.destroy() }
                        }
                    }
                }
            }
        }
    }

    private class PermissionBridge : HostBridge {
        val decisions = LinkedBlockingQueue<PermissionDecision>()
        val requests = LinkedBlockingQueue<RemotePermission>()
        private val memory = MemoryStorage()
        override val storage: HostStorage = memory
        override val coreStorage: HostCoreStorage = memory
        override suspend fun navigateTo(url: String) = Unit
        override suspend fun featureSupported(request: HostFeatureSupportedRequest) = false
        override suspend fun devicePermission(product: ProductExecutionConfig, request: HostDevicePermissionRequest) =
            PermissionDecision.DENY
        override suspend fun remotePermission(product: ProductExecutionConfig, request: RemotePermission): PermissionDecision {
            requests.add(request)
            return decisions.poll() ?: PermissionDecision.DENY
        }
    }

    private class MemoryStorage : HostStorage, HostCoreStorage {
        private val values = ConcurrentHashMap<Any, ByteArray>()
        override suspend fun read(key: String): ByteArray? = values[key]
        override suspend fun write(key: String, value: ByteArray) { values[key] = value }
        override suspend fun clear(key: String) { values.remove(key) }
        override suspend fun read(key: ByteArray): ByteArray? = values[key.toList()]
        override suspend fun write(key: ByteArray, value: ByteArray) { values[key.toList()] = value }
        override suspend fun clear(key: ByteArray) { values.remove(key.toList()) }
    }

    private class Reports {
        val ready = CountDownLatch(1)
        val reset = CountDownLatch(1)
        val results = LinkedBlockingQueue<String>()
        @JavascriptInterface fun ready() { ready.countDown() }
        @JavascriptInterface fun disconnected() { reset.countDown() }
        @JavascriptInterface fun result(value: String) { results.add(value) }
    }

    private class BridgeProxy(destination: Int) : Closeable {
        private val loopback = InetAddress.getByName("127.0.0.1")
        private val listener = ServerSocket(0, 10, loopback)
        private val executor = Executors.newCachedThreadPool()
        private val sockets = ConcurrentHashMap.newKeySet<Socket>()
        private val browser = AtomicReference<Socket>()
        val connections = AtomicInteger()
        val port: Int = listener.localPort

        init {
            executor.execute {
                while (!listener.isClosed) {
                    val incoming = runCatching { listener.accept() }.getOrNull() ?: break
                    val outgoing = Socket(loopback, destination)
                    sockets.addAll(listOf(incoming, outgoing))
                    browser.set(incoming)
                    connections.incrementAndGet()
                    relay(incoming, outgoing)
                    relay(outgoing, incoming)
                }
            }
        }

        private fun relay(source: Socket, target: Socket) {
            executor.execute {
                runCatching { source.getInputStream().copyTo(target.getOutputStream()) }
                runCatching { source.close() }
                runCatching { target.close() }
            }
        }

        fun disconnect() {
            browser.get().apply { setSoLinger(true, 0); close() }
        }

        override fun close() {
            listener.close()
            sockets.forEach { runCatching { it.close() } }
            executor.shutdownNow()
        }
    }

    private companion object {
        const val PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII="
        val PAGE = """
            <!doctype html><html><body><script>
            const host = window.__HOST_API_CLIENT__;
            const client = host.client;
            let connected = false;
            host.subscribeConnectionStatus(status => {
              if (status === 'connected') connected = true;
              if (connected && status === 'disconnected') httpReport.disconnected();
            });
            window.requestPermission = async () => {
              const result = await client.permissions.requestRemotePermission({
                permission: { tag: 'Remote', value: { domains: ['localhost'] } }
              });
              return result.isOk() && result.value.granted && host.client === client;
            };
            window.loadResource = (kind, url) => new Promise((resolve, reject) => {
              if (kind === 'fetch') { fetch(url).then(response => resolve(response.ok), reject); return; }
              if (kind === 'xhr') {
                const request = new XMLHttpRequest();
                request.open('GET', url);
                request.onload = () => resolve(request.status === 200);
                request.onerror = () => resolve(false);
                request.send();
                return;
              }
              const element = document.createElement(kind === 'image' ? 'img' : 'script');
              element.onload = () => resolve(true);
              element.onerror = () => resolve(false);
              element.src = url;
              document.body.appendChild(element);
            });
            httpReport.ready();
            </script></body></html>
        """.trimIndent()
    }
}
