package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import org.junit.Assert.assertEquals
import org.junit.Test

private const val PRODUCT = "overlay-test.dot"

class ProductOverlayPageTest {
    @Test
    fun `a page on the product origin is shown`() {
        assertEquals("https://overlay-test.dot/confetti-overlay.html", ProductOverlayPage.url("confetti-overlay.html", PRODUCT))
        assertEquals(
            "https://overlay-test.dot/fx/confetti-overlay.html?seed=1",
            ProductOverlayPage.url("fx/confetti-overlay.html?seed=1", PRODUCT)
        )
    }

    @Test
    fun `a path that names no file is refused`() {
        val paths = listOf("?x", "#x", "?seed=1#x", "confetti", "fx/", "fx/confetti", "fx.d/confetti", "confetti.")

        assertEquals(paths.map { null }, paths.map { ProductOverlayPage.url(it, PRODUCT) })
    }

    @Test
    fun `a path off the product origin is refused`() {
        val paths = listOf("", " ", "/confetti.html", "//evil.invalid/x.html", "https://evil.invalid/x.html", "../x.html", "javascript:x.js")

        assertEquals(paths.map { null }, paths.map { ProductOverlayPage.url(it, PRODUCT) })
    }
}
