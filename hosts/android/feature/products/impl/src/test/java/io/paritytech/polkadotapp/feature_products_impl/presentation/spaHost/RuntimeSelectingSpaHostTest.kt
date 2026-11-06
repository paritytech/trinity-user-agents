package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import com.google.gson.Gson
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.InMemoryBundledProductFiles
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.test.TestScope
import org.junit.Test

class RuntimeSelectingSpaHostTest {
    private val scope = object : ComputationalScope, CoroutineScope by TestScope() {}
    private val messageDisplay = mockk<MessageDisplay>(relaxed = true)
    private val session = mockk<SpaHostSession>()
    private val native = mockk<NativeSpaHost> {
        every { with(scope) { with(messageDisplay) { createSession(any(), any()) } } } returns session
    }
    private val truapi = mockk<TrUAPISpaHost> {
        every { with(scope) { with(messageDisplay) { createSession(any(), any()) } } } returns session
    }
    private val runtimeSettings = mockk<ProductRuntimeSettings> { every { isTrUAPIRuntimeEnabled() } returns false }
    private val bundledProducts = BundledProducts(
        InMemoryBundledProductFiles(mapOf("bundled-products/provider.dot/app/index.html" to "<html></html>")),
        ManifestParser(Gson()),
        enabled = true,
    )
    private val host = RuntimeSelectingSpaHost(native, truapi, runtimeSettings, bundledProducts)

    // Only TrUAPI serves a product the app ships, so it opens there with the TrUAPI runtime switched off.
    @Test
    fun `a bundled product opens on TrUAPI whatever the toggle`() {
        with(scope) { with(messageDisplay) { host.createSession("https://provider.dot/card", underCard = false) } }

        verify { with(scope) { with(messageDisplay) { truapi.createSession("https://provider.dot/card", false) } } }
        verify(exactly = 0) { with(scope) { with(messageDisplay) { native.createSession(any(), any()) } } }
    }

    @Test
    fun `any other product follows the toggle`() {
        with(scope) { with(messageDisplay) { host.createSession("https://coinflip.dot", underCard = false) } }

        verify { with(scope) { with(messageDisplay) { native.createSession("https://coinflip.dot", any()) } } }
        verify(exactly = 0) { with(scope) { with(messageDisplay) { truapi.createSession(any(), any()) } } }
    }
}
