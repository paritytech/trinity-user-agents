package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPISessionStarter
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.test.TestScope
import org.junit.Test
import uniffi.truapi.DevServerProduct

class ProductTabSessionFactoryTest {
    private val devServer = DevServerProduct(productId = "localhost:3000", origin = "http://192.168.1.59:3000")
    private val provider = mockk<BrowserWebViewProvider>()
    private val webViewProviders = mockk<BrowserWebViewProvider.Factory> {
        every { create(any(), any(), any(), any(), any(), any(), any()) } returns provider
    }
    private val sessionStarter = mockk<TrUAPISessionStarter>(relaxed = true)
    private val native = mockk<NativeProductTabSessionFactory> {
        every { create(any(), any(), any()) } returns provider
    }
    private val runtimeSettings = mockk<ProductRuntimeSettings> { every { isTrUAPIRuntimeEnabled() } returns false }
    private val devServers = mockk<DevServers> { every { productAt(any()) } returns null }
    private val scope = TestScope()

    private val factory = ProductTabSessionFactory(
        native,
        TrUAPIProductTabSessionFactory(webViewProviders, sessionStarter, mockk(relaxed = true)),
        runtimeSettings,
        devServers,
    )

    @Test
    fun `a dev server runs on TrUAPI under its localhost id, with the address it loads from as first party`() {
        // The core runs a localhost product whichever runtime the toggle selects, and a phone reaches the
        // server by an address that is not that id, so neither can be derived from the other.
        every { devServers.productAt(devServer.origin) } returns devServer

        factory.create(devServer.origin, scope) {}

        val productId = ProductId.fromStoredValue("localhost:3000")
        verify {
            webViewProviders.create(devServer.origin, any(), true, scope, productId, devServer.origin, any())
            sessionStarter.start(provider, devServer.origin, scope, any(), null, productId)
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
