package io.paritytech.polkadotapp.feature_products_api.presentation.deeplink

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductDeepLinkGateTest {
    // The flag is what keeps an unvetted product off the screen: a link that resolves its manifest
    // also runs its worker and hosts its pages, which is exactly what a safety build refuses.
    @Test
    fun `off the flag, no product deeplink opens`() {
        assertFalse(ProductDeepLinkGate(arbitraryProductsEnabled = false).opens())
    }

    @Test
    fun `on the flag, product deeplinks open`() {
        assertTrue(ProductDeepLinkGate(arbitraryProductsEnabled = true).opens())
    }
}
