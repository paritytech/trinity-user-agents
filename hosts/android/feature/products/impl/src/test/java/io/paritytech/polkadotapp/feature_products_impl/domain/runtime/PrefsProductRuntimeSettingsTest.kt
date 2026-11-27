package io.paritytech.polkadotapp.feature_products_impl.domain.runtime

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.DevServerProduct
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource

class PrefsProductRuntimeSettingsTest {
    private val prefs = MapSharedPreferences()

    @Test
    fun `defaults to the TrUAPI runtime`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = true)

        assertTrue(settings.isTrUAPIRuntimeEnabled())
    }

    @Test
    fun `persists an opt-out to the native host`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = true)

        settings.setTrUAPIRuntimeEnabled(false)

        assertFalse(settings.isTrUAPIRuntimeEnabled())
        assertFalse(PrefsProductRuntimeSettings(prefs, isDebugBuild = true).isTrUAPIRuntimeEnabled())
    }

    @Test
    fun `release builds run native despite the TrUAPI default`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = false)

        assertFalse(settings.isTrUAPIRuntimeEnabled())

        settings.setTrUAPIRuntimeEnabled(true)

        assertFalse(settings.isTrUAPIRuntimeEnabled())
    }

    // Only the core gives a development server product its id, so the toggle does not apply.
    @Test
    fun `a dev server product runs on TrUAPI whatever the toggle`() {
        val settings = PrefsProductRuntimeSettings(prefs, isDebugBuild = false)
        val productId = ProductId.parse("provider.dot").getOrThrow()
        val devServer = ProductSource.DevServer(DevServerProduct(productId, "http://10.0.2.2:3000", "http://10.0.2.2:3000/"))

        assertTrue(settings.isTrUAPIRuntimeEnabledFor(devServer))
        assertFalse(settings.isTrUAPIRuntimeEnabledFor(null))
    }
}
