package io.paritytech.polkadotapp.app

import android.Manifest
import android.net.Uri
import android.webkit.JavascriptInterface
import android.webkit.PermissionRequest
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.parity.truapi.HostBridge
import io.parity.truapi.HostCoreStorage
import uniffi.truapi.HostRuntimeConfig
import io.parity.truapi.HostStorage
import io.parity.truapi.LocalhostBridgeBootstrap
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductExecutionKind
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIBootstrapInstaller
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
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
import java.lang.reflect.Proxy
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.UUID
import kotlin.io.path.createTempDirectory

@RunWith(AndroidJUnit4::class)
class MediaPermissionIntegrationTest {
    @Test
    fun productRawMediaRemainsDeniedDespiteDeviceAndGenericNetworkGrants() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        for (permission in listOf(Manifest.permission.CAMERA, Manifest.permission.RECORD_AUDIO)) {
            instrumentation.uiAutomation.grantRuntimePermission(context.packageName, permission)
        }
        val bridge = PermissionBridge()
        val reports = Reports()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
        val config = HostRuntimeConfig(
            hostName = "Media isolation test",
            peopleChainGenesisHash = ByteArray(32),
            bulletinChainGenesisHash = ByteArray(32),
            assetHubChainGenesisHash = ByteArray(32),
            networkSuffix = "paseo",
            databaseDirectory = createTempDirectory("truapi").toString(),
        )
        TrUAPIHostRuntime(bridge, config).use { runtime ->
            runtime.openProductExecution(bridge, ProductExecutionConfig("media.paseo", ProductExecutionKind.APP)).use { execution ->
                for (capability in listOf(HostDevicePermissionRequest.CAMERA, HostDevicePermissionRequest.MICROPHONE)) {
                    execution.setPermissionAuthorizationStatus(PermissionAuthorizationRequest.Device(capability), PermissionAuthorizationStatus.AUTHORIZED)
                }
                execution.setPermissionAuthorizationStatus(
                    PermissionAuthorizationRequest.Remote(RemotePermissionRequest(RemotePermission.WebRtc)),
                    PermissionAuthorizationStatus.AUTHORIZED,
                )
                val endpoint = execution.startWsBridge()
                lateinit var webView: WebView
                instrumentation.runOnMainSync {
                    webView = WebView(context)
                    webView.settings.javaScriptEnabled = true
                    val chrome = ProductWebChromeClient(unused(), unused(), "Media test",
                        FixedProductId(ProductId.fromStoredValue("media.paseo")), scope, null)
                    webView.webChromeClient = chrome
                    var nativeDenied = false
                    chrome.onPermissionRequest(object : PermissionRequest() {
                        override fun getOrigin(): Uri = Uri.parse("http://localhost")
                        override fun getResources() = arrayOf(RESOURCE_VIDEO_CAPTURE, RESOURCE_AUDIO_CAPTURE)
                        override fun grant(resources: Array<out String>) { error("Raw native capture was granted") }
                        override fun deny() { nativeDenied = true }
                    })
                    assertTrue("Native capture must be denied independently of script policy", nativeDenied)
                    webView.addJavascriptInterface(reports, "mediaReport")
                    webView.webViewClient = object : WebViewClient() {
                        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest) =
                            WebResourceResponse("text/html", "UTF-8", PAGE.byteInputStream())
                    }
                    TrUAPIBootstrapInstaller(context).installerFor(setOf("http://localhost"))(
                        LocalhostBridgeBootstrap.script(endpoint.port, endpoint.token),
                    )(webView)
                    webView.loadUrl("http://localhost/")
                }
                try {
                    assertTrue("fixture did not load", reports.ready.await(15, TimeUnit.SECONDS))
                    instrumentation.runOnMainSync {
                        webView.evaluateJavascript("probe().then(value => mediaReport.result(value)).catch(error => mediaReport.result(String(error)));", null)
                    }
                    assertEquals("camera:denied,microphone:denied,display:denied,rtc:denied,fullscreen:denied",
                        reports.results.poll(15, TimeUnit.SECONDS))
                } finally {
                    instrumentation.runOnMainSync { webView.destroy() }
                    scope.cancel()
                }
            }
        }
    }

    private class PermissionBridge : HostBridge {
        override val storage: HostStorage = unused()
        override val coreStorage = object : HostCoreStorage {
            override val storageIdentifier = "media-isolation-${UUID.randomUUID()}"
            private val values = ConcurrentHashMap<List<Byte>, ByteArray>()
            override suspend fun read(key: ByteArray): ByteArray? = values[key.toList()]?.copyOf()
            override suspend fun write(key: ByteArray, value: ByteArray) { values[key.toList()] = value.copyOf() }
            override suspend fun clear(key: ByteArray) { values.remove(key.toList()) }
        }
        override suspend fun navigateTo(url: String) = Unit
        override suspend fun featureSupported(request: HostFeatureSupportedRequest) = false
        override suspend fun remotePermission(product: ProductExecutionConfig, request: RemotePermission) = PermissionDecision.DENY
        override suspend fun devicePermission(product: ProductExecutionConfig, request: HostDevicePermissionRequest): PermissionDecision =
            error("Raw capture must never prompt through Core Media")
    }

    private class Reports {
        val ready = CountDownLatch(1)
        val results = LinkedBlockingQueue<String>()
        @JavascriptInterface fun ready() { ready.countDown() }
        @JavascriptInterface fun result(value: String) { results.add(value) }
    }

    companion object {
        @Suppress("UNCHECKED_CAST")
        private inline fun <reified T> unused(): T = Proxy.newProxyInstance(
            T::class.java.classLoader, arrayOf(T::class.java),
        ) { _, method, _ -> error("Unexpected ${T::class.java.simpleName}.${method.name}") } as T

        private const val PAGE = """
            <!doctype html><html><script>
            async function probe() {
              const results = [];
              for (const [name, operation] of [
                ['camera', () => navigator.mediaDevices.getUserMedia({video: true})],
                ['microphone', () => navigator.mediaDevices.getUserMedia({audio: true})],
                ['display', () => navigator.mediaDevices.getDisplayMedia({video: true})],
                ['rtc', () => new RTCPeerConnection()],
                ['fullscreen', () => document.documentElement.requestFullscreen()]
              ]) {
                try { const handle = await operation(); if (handle?.getTracks) handle.getTracks().forEach(track => track.stop()); if (handle?.close) handle.close(); results.push(name + ':allowed'); }
                catch (_) { results.push(name + ':denied'); }
              }
              return results.join(',');
            }
            mediaReport.ready();
            </script></html>
        """
    }
}
