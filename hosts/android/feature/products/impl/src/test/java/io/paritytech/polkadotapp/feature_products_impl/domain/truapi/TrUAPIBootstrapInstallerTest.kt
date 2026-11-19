package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.content.res.AssetManager
import android.net.Uri
import android.os.Build
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_products_impl.BuildConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.mockStatic
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions
import uniffi.truapi.truapiHostVersion
import java.io.ByteArrayInputStream
import java.io.FileNotFoundException

class TrUAPIBootstrapInstallerTest {
    private val webView = mock(WebView::class.java)
    private val context = mock(Context::class.java)
    private val assets = mock(AssetManager::class.java)
    private val chromeClient = mock(ProductWebChromeClient::class.java)
    private val productOrigins = setOf("https://product.paseo")

    @Test
    fun `endpoint is restricted to the product main frame while every frame gets the container`() {
        withContainerAsset()
        mockStatic(WebViewFeature::class.java).use { features ->
            features.`when`<Boolean> { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) }.thenReturn(true)
            mockStatic(Uri::class.java).use {
                mockStatic(WebViewCompat::class.java).use { compat ->
                    val registered = mutableListOf<Registration>()
                    compat.`when`<Any> { WebViewCompat.addDocumentStartJavaScript(eq(webView), any(), any()) }
                        .thenAnswer { call ->
                            registered += Registration(call.getArgument(1), call.getArgument(2))
                            null
                        }

                    TrUAPIBootstrapInstaller(context)
                        .installerFor(productOrigins, "endpoint-test-session")("endpoint();")(webView)

                    assertEquals(
                        listOf(
                            Registration("if (window === window.top) {\nendpoint();\n}", productOrigins),
                            Registration(
                                "window.__truapi_localhost = {...window.__truapi_localhost, nativeHttp: true};\ncontainer();",
                                setOf("*"),
                            ),
                        ),
                        registered,
                    )
                    verify(chromeClient).useContainerPermissions()
                }
            }
        }
    }

    @Test
    fun `host telemetry and endpoint share the main frame guard for every webview`() {
        assumeTrue(FeatureOption.HOST_TELEMETRY.isEnabled)
        withContainerAsset()
        mockStatic(WebViewFeature::class.java).use { features ->
            features.`when`<Boolean> { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) }.thenReturn(true)
            mockStatic(Uri::class.java).use {
                mockStatic(WebViewCompat::class.java).use { compat ->
                    val registered = mutableListOf<Registration>()
                    compat.`when`<Any> { WebViewCompat.addDocumentStartJavaScript(eq(webView), any(), any()) }
                        .thenAnswer { call ->
                            registered += Registration(call.getArgument(1), call.getArgument(2))
                            null
                        }

                    withTelemetryMetadata()
                    val installer = TrUAPIBootstrapInstaller(context)
                        .installerFor(productOrigins, "telemetry-test-session")("endpoint();")
                    installer(webView)
                    installer(webView)

                    assertEquals(4, registered.size)
                    assertTrue(registered.first().script.startsWith("if (window === window.top) {"))
                    assertTrue(registered.first().script, registered.first().script.contains("try {"))
                    assertTrue(registered.first().script.contains("\"hostSessionId\":\"telemetry-test-session\""))
                    assertTrue(registered.first().script.endsWith("endpoint();\n}"))
                    assertEquals(registered.take(2), registered.drop(2))
                }
            }
        }
    }

    @Test
    fun `host telemetry script exposes build identity as a frozen non replaceable global`() {
        val packageInfo = mock(PackageInfo::class.java).apply { versionName = "1.2.3" }
        val packageManager = mock(PackageManager::class.java)
        whenever(packageInfo.longVersionCode).thenReturn(456L)
        whenever(context.packageName).thenReturn("io.paritytech.polkadotapp.nightly")
        whenever(context.packageManager).thenReturn(packageManager)
        whenever(packageManager.getPackageInfo("io.paritytech.polkadotapp.nightly", 0)).thenReturn(packageInfo)

        val script = hostTelemetryScript(
            context = context,
            hostSessionId = "123e4567-e89b-12d3-a456-426614174000",
        )

        assertTrue(script.contains("try {"))
        assertTrue(script.contains("if (!Object.prototype.hasOwnProperty.call(globalThis, \"__trinityHostTelemetry\"))"))
        assertTrue(script.contains("Object.defineProperty(globalThis, \"__trinityHostTelemetry\""))
        assertTrue(script.contains("value: Object.freeze({\"hostSessionId\":\"123e4567-e89b-12d3-a456-426614174000\""))
        assertTrue(script.contains("\"appVersion\":\"1.2.3\""))
        assertTrue(script.contains("\"appBuild\":\"456\""))
        assertTrue(script.contains("\"appVariant\":\"${BuildConfig.BUILD_TYPE}\""))
        assertTrue(script.contains("\"platform\":\"android\""))
        Build.VERSION.RELEASE?.let { assertTrue(script.contains("\"osVersion\":\"$it\"")) }
        assertTrue(script.contains("\"truapiHostVersion\":\"${truapiHostVersion()}\""))
        assertTrue(script.contains("\"schemaVersion\":\"1\""))
        assertTrue(script.contains("writable: false"))
        assertTrue(script.contains("configurable: false"))
        assertTrue(script.endsWith("} catch {}"))
    }

    @Test
    fun `telemetry construction failure cannot prevent endpoint installation`() {
        withContainerAsset()
        whenever(context.packageName).thenReturn("io.paritytech.polkadotapp.nightly")
        mockStatic(WebViewFeature::class.java).use { features ->
            features.`when`<Boolean> { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) }.thenReturn(true)
            mockStatic(Uri::class.java).use {
                mockStatic(WebViewCompat::class.java).use { compat ->
                    val registered = mutableListOf<Registration>()
                    compat.`when`<Any> { WebViewCompat.addDocumentStartJavaScript(eq(webView), any(), any()) }
                        .thenAnswer { call ->
                            registered += Registration(call.getArgument(1), call.getArgument(2))
                            null
                        }

                    TrUAPIBootstrapInstaller(context)
                        .installerFor(productOrigins, "123e4567-e89b-12d3-a456-426614174000")("endpoint();")(webView)

                    assertEquals("if (window === window.top) {\nendpoint();\n}", registered.first().script)
                }
            }
        }
    }

    @Test
    fun `missing container fails before an execution can be opened`() {
        withContainerAsset()
        whenever(assets.open("truapi-container.js")).thenThrow(FileNotFoundException("missing container"))
        mockStatic(WebViewFeature::class.java).use { features ->
            features.`when`<Boolean> { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) }.thenReturn(true)

            assertThrows(FileNotFoundException::class.java) {
                TrUAPIBootstrapInstaller(context).installerFor(productOrigins, "missing-container-test")
            }
            verifyNoInteractions(chromeClient)
        }
    }

    @Test
    fun `unsupported document start fails before an execution can be opened`() {
        mockStatic(WebViewFeature::class.java).use { features ->
            features.`when`<Boolean> { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) }.thenReturn(false)

            assertThrows(IllegalStateException::class.java) {
                TrUAPIBootstrapInstaller(context).installerFor(productOrigins, "unsupported-webview-test")
            }
        }
    }

    private fun withContainerAsset() {
        whenever(webView.webChromeClient).thenReturn(chromeClient)
        whenever(context.assets).thenReturn(assets)
        whenever(assets.open("truapi-container.js")).thenReturn(ByteArrayInputStream("container();".toByteArray()))
    }

    private fun withTelemetryMetadata() {
        val packageInfo = mock(PackageInfo::class.java).apply { versionName = "1.2.3" }
        val packageManager = mock(PackageManager::class.java)
        whenever(packageInfo.longVersionCode).thenReturn(456L)
        whenever(context.packageName).thenReturn("io.paritytech.polkadotapp.nightly")
        whenever(context.packageManager).thenReturn(packageManager)
        whenever(packageManager.getPackageInfo("io.paritytech.polkadotapp.nightly", 0)).thenReturn(packageInfo)
    }

    private data class Registration(val script: String, val origins: Set<String>)
}
