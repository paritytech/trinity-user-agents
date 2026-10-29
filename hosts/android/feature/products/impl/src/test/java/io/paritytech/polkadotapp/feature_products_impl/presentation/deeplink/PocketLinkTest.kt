package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PocketLinkTest {
    /** A product's `navigateTo`, a QR code and another app all hand over the same link, under the app's scheme. */
    @Test
    fun `a Pocket link is read under the app's scheme`() {
        assertTrue(isPocketLink(scheme = "polkadotapp", host = "pocket"))
    }

    /**
     * A deeplink's host is lowercase, and the app matches it exactly, as it does chat, pay and pair, so it reads a
     * Pocket link the way iOS does.
     */
    @Test
    fun `a mixed-case host is not the Pocket`() {
        assertFalse(isPocketLink(scheme = "polkadotapp", host = "Pocket"))
    }

    /** Under a web scheme `pocket` is an ordinary host, and the products' own scheme names products, not the Pocket. */
    @Test
    fun `pocket under another scheme is not the Pocket`() {
        assertFalse(isPocketLink(scheme = "https", host = "pocket"))
        assertFalse(isPocketLink(scheme = "polkadot", host = "pocket"))
    }

    /** A product's paths are its own App routes, whatever they spell, so the host claims none of them. */
    @Test
    fun `a link to a product is never a Pocket link`() {
        assertFalse(isPocketLink(scheme = "polkadotapp", host = "game.dot"))
    }
}
