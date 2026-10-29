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
import io.mockk.Called
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_products_impl.BuildConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import uniffi.truapi.truapiHostVersion
import java.io.ByteArrayInputStream
import java.io.FileNotFoundException

class TrUAPIBootstrapInstallerTest {
    private val webView = mockk<WebView>()
    private val context = mockk<Context>()
    private val assets = mockk<AssetManager>()
    private val chromeClient = mockk<ProductWebChromeClient>(relaxUnitFun = true)
    private val productOrigins = setOf("https://product.paseo")

    @Test
    fun `endpoint is restricted to the product main frame while every frame gets the container`() {
        withContainerAsset()
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns true
            mockkStatic(Uri::class) {
                every { Uri.parse(any()) } returns null
                mockkStatic(WebViewCompat::class) {
                    val registered = mutableListOf<Registration>()
                    every { WebViewCompat.addDocumentStartJavaScript(webView, any(), any()) } answers {
                        registered += Registration(secondArg(), thirdArg())
                        mockk()
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
                    verify(exactly = 1) { chromeClient.useContainerPermissions() }
                }
            }
        }
    }

    @Test
    fun `host telemetry and endpoint share the main frame guard for every webview`() {
        assumeTrue(FeatureOption.HOST_TELEMETRY.isEnabled)
        withContainerAsset()
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns true
            mockkStatic(Uri::class) {
                every { Uri.parse(any()) } returns null
                mockkStatic(WebViewCompat::class) {
                    val registered = mutableListOf<Registration>()
                    every { WebViewCompat.addDocumentStartJavaScript(webView, any(), any()) } answers {
                        registered += Registration(secondArg(), thirdArg())
                        mockk()
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
        val packageInfo = mockk<PackageInfo>().apply { versionName = "1.2.3" }
        val packageManager = mockk<PackageManager>()
        every { packageInfo.longVersionCode } returns 456L
        every { context.packageName } returns "io.paritytech.polkadotapp.nightly"
        every { context.packageManager } returns packageManager
        every { packageManager.getPackageInfo("io.paritytech.polkadotapp.nightly", 0) } returns packageInfo

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
        every { context.packageName } returns "io.paritytech.polkadotapp.nightly"
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns true
            mockkStatic(Uri::class) {
                every { Uri.parse(any()) } returns null
                mockkStatic(WebViewCompat::class) {
                    val registered = mutableListOf<Registration>()
                    every { WebViewCompat.addDocumentStartJavaScript(webView, any(), any()) } answers {
                        registered += Registration(secondArg(), thirdArg())
                        mockk()
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
        every { assets.open("truapi-container.js") } throws FileNotFoundException("missing container")
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns true

            assertThrows(FileNotFoundException::class.java) {
                TrUAPIBootstrapInstaller(context).installerFor(productOrigins, "missing-container-test")
            }
            verify { chromeClient wasNot Called }
        }
    }

    @Test
    fun `unsupported document start fails before an execution can be opened`() {
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns false

            assertThrows(IllegalStateException::class.java) {
                TrUAPIBootstrapInstaller(context).installerFor(productOrigins, "unsupported-webview-test")
            }
        }
    }

    private fun withContainerAsset() {
        every { webView.webChromeClient } returns chromeClient
        every { context.assets } returns assets
        every { assets.open("truapi-container.js") } returns ByteArrayInputStream("container();".toByteArray())
    }

    private fun withTelemetryMetadata() {
        val packageInfo = mockk<PackageInfo>().apply { versionName = "1.2.3" }
        val packageManager = mockk<PackageManager>()
        every { packageInfo.longVersionCode } returns 456L
        every { context.packageName } returns "io.paritytech.polkadotapp.nightly"
        every { context.packageManager } returns packageManager
        every { packageManager.getPackageInfo("io.paritytech.polkadotapp.nightly", 0) } returns packageInfo
    }

    private data class Registration(val script: String, val origins: Set<String>)
}
