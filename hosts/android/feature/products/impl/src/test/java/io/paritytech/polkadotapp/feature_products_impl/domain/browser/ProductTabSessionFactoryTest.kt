package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.bundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.productSources
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPISessionStarter
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.test.TestScope
import org.junit.Test
import uniffi.truapi.DevServerProduct
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource

class ProductTabSessionFactoryTest {
    private val devServer = DevServerProduct(
        productId = ProductId.parse("humanity.dot").getOrThrow(),
        origin = "http://192.168.1.59:3000",
        url = "http://192.168.1.59:3000/app?product_id=humanity.dot",
    )
    private val provider = mockk<BrowserWebViewProvider>()
    private val webViewProviders = mockk<BrowserWebViewProvider.Factory> {
        every { create(any(), any(), any(), any(), any(), any(), any()) } returns provider
    }
    private val sessionStarter = mockk<TrUAPISessionStarter>(relaxed = true)
    private val native = mockk<NativeProductTabSessionFactory> {
        every { create(any(), any(), any()) } returns provider
    }
    private val runtimeSettings = mockk<ProductRuntimeSettings> { every { isTrUAPIRuntimeEnabled() } returns false }
    private val scope = TestScope()
    private val bundledProducts = bundledProducts(
        mapOf(
            "bundled-products/provider.dot/worker/index.js" to "export {}",
            "bundled-products/provider.dot/app/index.html" to "<html></html>",
        ),
    )

    private val factory = ProductTabSessionFactory(
        native,
        TrUAPIProductTabSessionFactory(webViewProviders, sessionStarter, mockk(relaxed = true)),
        runtimeSettings,
        productSources(bundledProducts),
    )

    @Test
    fun `a dev server runs on TrUAPI under the core's id, with the server's origin as first party`() {
        // The core runs a dev server product whichever runtime the toggle selects, and a phone reaches the
        // server by an address that is not that id, so neither can be derived from the other.
        factory.create(devServer.url, scope) {}

        val productId = ProductId.parse("humanity.dot").getOrThrow()
        verify {
            webViewProviders.create(
                devServer.url,
                any(),
                true,
                scope,
                source = ProductSource.DevServer(devServer),
                servedExecutable = any(),
            )
            sessionStarter.start(provider, devServer.url, scope, any(), null, productId)
        }
        verify(exactly = 0) { native.create(any(), any(), any()) }
    }

    // A tab opened on a product the app ships serves it from its own files, as a sheet does, and only
    // TrUAPI serves it, so the runtime toggle does not apply.
    @Test
    fun `a bundled product's page runs on TrUAPI from its own files under its own id`() {
        val url = "https://provider.dot/card"
        val product = bundledProducts.entry(ProductId.parse("provider.dot").getOrThrow())!!

        factory.create(url, scope) {}

        verify {
            webViewProviders.create(
                url,
                any(),
                true,
                scope,
                source = ProductSource.Bundled(product),
                servedExecutable = any(),
            )
            sessionStarter.start(provider, url, scope, any(), null, product.productId)
        }
        verify(exactly = 0) { native.create(any(), any(), any()) }
    }

    @Test
    fun `any other page follows the runtime toggle`() {
        factory.create("https://coinflip.dot.li", scope) {}

        verify { native.create("https://coinflip.dot.li", scope, any()) }
        verify(exactly = 0) { sessionStarter.start(any(), any(), any(), any(), any(), any()) }
    }
}
