package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_impl.domain.browser.productSources
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
        every { with(scope) { with(messageDisplay) { createSession(any(), any(), any()) } } } returns session
    }
    private val runtimeSettings = object : ProductRuntimeSettings {
        override fun isTrUAPIRuntimeEnabled() = false

        override fun setTrUAPIRuntimeEnabled(enabled: Boolean) = Unit
    }
    private val host = RuntimeSelectingSpaHost(native, truapi, runtimeSettings, productSources())

    // Only TrUAPI gives a dev server page its product id, so it opens there with the TrUAPI runtime switched off.
    @Test
    fun `a dev server page opens on TrUAPI whatever the toggle`() {
        with(scope) { with(messageDisplay) { host.createSession(DEV_SERVER_URL, underCard = false) } }

        verify { with(scope) { with(messageDisplay) { truapi.createSession(DEV_SERVER_URL, false, any()) } } }
        verify(exactly = 0) { with(scope) { with(messageDisplay) { native.createSession(any(), any()) } } }
    }

    @Test
    fun `any other product follows the toggle`() {
        with(scope) { with(messageDisplay) { host.createSession("https://coinflip.dot", underCard = false) } }

        verify { with(scope) { with(messageDisplay) { native.createSession("https://coinflip.dot", any()) } } }
        verify(exactly = 0) { with(scope) { with(messageDisplay) { truapi.createSession(any(), any(), any()) } } }
    }

    private companion object {
        const val DEV_SERVER_URL = "http://192.168.1.59:3000/app?product_id=humanity.dot"
    }
}
