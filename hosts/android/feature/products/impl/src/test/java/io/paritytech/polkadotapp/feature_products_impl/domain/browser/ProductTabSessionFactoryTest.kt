package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
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
    private val scope = TestScope()
    private val factory = ProductTabSessionFactory(
        webViewProviders,
        sessionStarter,
        mockk(relaxed = true),
        productSources(),
    )

    @Test
    fun `a dev server runs under the core's id, with the server's origin as first party`() {
        // A phone reaches the server by an address that is not that id, so neither can be derived from the other.
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
    }

    @Test
    fun `a published page reads its id off each url it loads`() {
        factory.create("https://coinflip.dot.li", scope) {}

        verify { sessionStarter.start(provider, "https://coinflip.dot.li", scope, any(), null, null) }
    }
}
