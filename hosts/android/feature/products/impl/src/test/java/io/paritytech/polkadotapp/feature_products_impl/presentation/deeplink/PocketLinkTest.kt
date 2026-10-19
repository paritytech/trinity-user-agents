package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PocketLinkTest {
    /**
     * A product's `navigateTo` hands the link over under the core's scheme, while a QR code or another app opens it
     * under the app's own. Both have to reach the Pocket.
     */
    @Test
    fun `a Pocket link is read under the core's scheme and the app's own`() {
        assertTrue(isPocketLink(scheme = "polkadot", host = "pocket"))
        assertTrue(isPocketLink(scheme = "polkadotapp", host = "pocket"))
    }

    @Test
    fun `pocket under a web scheme is an ordinary host`() {
        assertFalse(isPocketLink(scheme = "https", host = "pocket"))
    }

    /** A product's paths are its own App routes, whatever they spell, so the host claims none of them. */
    @Test
    fun `a link to a product is never a Pocket link`() {
        assertFalse(isPocketLink(scheme = "polkadotapp", host = "game.dot"))
    }
}
