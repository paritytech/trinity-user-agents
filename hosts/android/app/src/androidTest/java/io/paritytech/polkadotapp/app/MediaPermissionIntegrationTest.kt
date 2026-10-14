package io.paritytech.polkadotapp.app

import android.Manifest
import android.webkit.JavascriptInterface
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
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.DeviceCapabilityPermissionHandler
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
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus
import uniffi.truapi.PermissionDecision
import java.lang.reflect.Proxy
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.io.path.createTempDirectory

@RunWith(AndroidJUnit4::class)
class MediaPermissionIntegrationTest {
    @Test
    fun rustConsentPrecedesNativeCaptureAndAllowOnceIsConsumedOnce() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        for (permission in listOf(Manifest.permission.CAMERA, Manifest.permission.RECORD_AUDIO)) {
            instrumentation.uiAutomation.grantRuntimePermission(context.packageName, permission)
        }
        val bridge = PermissionBridge()
        val reports = Reports()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
        val permissionAsker = object : PermissionAsker by unused() {
            override suspend fun askPermission(vararg permissions: String): PermissionResult {
                bridge.events.add("native:${permissions.single()}")
                return PermissionResult.GRANTED
            }
        }
        val config = HostRuntimeConfig(
            hostName = "Media permission test",
            peopleChainGenesisHash = ByteArray(32),
            bulletinChainGenesisHash = ByteArray(32),
            assetHubChainGenesisHash = ByteArray(32),
            networkSuffix = "paseo",
            databaseDirectory = createTempDirectory("truapi").toString(),
        )
        TrUAPIHostRuntime(bridge, config).use { runtime ->
            runtime.openProductExecution(bridge, ProductExecutionConfig("media.paseo", ProductExecutionKind.APP)).use { execution ->
                val endpoint = execution.startWsBridge()
                lateinit var webView: WebView
                instrumentation.runOnMainSync {
                    webView = WebView(context)
                    webView.settings.javaScriptEnabled = true
                    webView.webChromeClient = ProductWebChromeClient(
                        unused(), DeviceCapabilityPermissionHandler(unused(), unused(), permissionAsker),
                        unused(), unused(), "Media test", FixedProductId(ProductId.fromStoredValue("media.paseo")), scope, null,
                    )
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
                fun call(script: String, expected: String) {
                    instrumentation.runOnMainSync {
                        webView.evaluateJavascript(
                            "Promise.resolve().then(() => $script).then(value => mediaReport.result(String(value)))" +
                                ".catch(error => mediaReport.result(error.name));",
                            null,
                        )
                    }
                    assertEquals(script, expected, reports.results.poll(15, TimeUnit.SECONDS))
                }
                try {
                    assertTrue("fixture did not load", reports.ready.await(15, TimeUnit.SECONDS))
                    for ((capability, manifest) in listOf(
                        HostDevicePermissionRequest.CAMERA to Manifest.permission.CAMERA,
                        HostDevicePermissionRequest.MICROPHONE to Manifest.permission.RECORD_AUDIO,
                    )) {
                        bridge.events.clear()
                        val name = if (capability == HostDevicePermissionRequest.CAMERA) "Camera" else "Microphone"
                        bridge.decisions.add(PermissionDecision.DENY)
                        call("capture('$name')", "NotAllowedError")
                        assertEquals(listOf("prompt:$capability"), bridge.events.toList())

                        execution.setPermissionAuthorizationStatus(
                            PermissionAuthorizationRequest.Device(capability), PermissionAuthorizationStatus.NOT_DETERMINED,
                        )
                        bridge.decisions.add(PermissionDecision.ALLOW_ONCE)
                        call(
                            "window.__HOST_API_CLIENT__.client.permissions.requestDevicePermission('$name')" +
                                ".match(value => value.granted, error => { throw error; })",
                            "true",
                        )
                        call("capture('$name')", "captured")
                        assertEquals(
                            listOf("prompt:$capability", "prompt:$capability", "native:$manifest"),
                            bridge.events.toList(),
                        )
                        bridge.decisions.add(PermissionDecision.DENY)
                        call("capture('$name')", "NotAllowedError")
                        assertEquals(
                            listOf("prompt:$capability", "prompt:$capability", "native:$manifest", "prompt:$capability"),
                            bridge.events.toList(),
                        )
                    }
                } finally {
                    instrumentation.runOnMainSync { webView.destroy() }
                    scope.cancel()
                }
            }
        }
    }

    private class PermissionBridge : HostBridge {
        val decisions = LinkedBlockingQueue<PermissionDecision>()
        val events = LinkedBlockingQueue<String>()
        override val storage: HostStorage = unused()
        override val coreStorage = object : HostCoreStorage {
            private val values = ConcurrentHashMap<List<Byte>, ByteArray>()
            override suspend fun read(key: ByteArray): ByteArray? = values[key.toList()]
            override suspend fun write(key: ByteArray, value: ByteArray) { values[key.toList()] = value }
            override suspend fun clear(key: ByteArray) { values.remove(key.toList()) }
        }
        override suspend fun navigateTo(url: String) = Unit
        override suspend fun featureSupported(request: HostFeatureSupportedRequest) = false
        override suspend fun remotePermission(product: ProductExecutionConfig, request: RemotePermission) = PermissionDecision.DENY
        override suspend fun devicePermission(
            product: ProductExecutionConfig,
            request: HostDevicePermissionRequest,
        ): PermissionDecision {
            events.add("prompt:$request")
            return checkNotNull(decisions.poll()) { "Unexpected extra permission prompt" }
        }
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
            async function capture(capability) {
              const stream = await navigator.mediaDevices.getUserMedia({
                video: capability === 'Camera', audio: capability === 'Microphone'
              });
              stream.getTracks().forEach(track => track.stop());
              return 'captured';
            }
            mediaReport.ready();
            </script></html>
        """
    }
}
