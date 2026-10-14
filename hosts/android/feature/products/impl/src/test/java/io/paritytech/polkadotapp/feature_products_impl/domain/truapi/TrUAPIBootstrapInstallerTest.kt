package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import android.content.res.AssetManager
import android.net.Uri
import android.webkit.WebView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.mockk.Called
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductWebChromeClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
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

                    TrUAPIBootstrapInstaller(context).installerFor(productOrigins)("endpoint();")(webView)

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
    fun `missing container fails before an execution can be opened`() {
        withContainerAsset()
        every { assets.open("truapi-container.js") } throws FileNotFoundException("missing container")
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns true

            assertThrows(FileNotFoundException::class.java) {
                TrUAPIBootstrapInstaller(context).installerFor(productOrigins)
            }
            verify { chromeClient wasNot Called }
        }
    }

    @Test
    fun `unsupported document start fails before an execution can be opened`() {
        mockkStatic(WebViewFeature::class) {
            every { WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT) } returns false

            assertThrows(IllegalStateException::class.java) {
                TrUAPIBootstrapInstaller(context).installerFor(productOrigins)
            }
        }
    }

    private fun withContainerAsset() {
        every { webView.webChromeClient } returns chromeClient
        every { context.assets } returns assets
        every { assets.open("truapi-container.js") } returns ByteArrayInputStream("container();".toByteArray())
    }

    private data class Registration(val script: String, val origins: Set<String>)
}
