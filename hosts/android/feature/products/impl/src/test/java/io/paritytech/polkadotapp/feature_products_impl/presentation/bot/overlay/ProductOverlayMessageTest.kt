package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import org.junit.Assert.assertEquals
import org.junit.Test

class ProductOverlayMessageTest {
    @Test
    fun `the page says it is done`() {
        assertEquals(ProductOverlayMessage.Done, ProductOverlayMessage.parse("""{"kind":"overlayDone"}"""))
        assertEquals(ProductOverlayMessage.Done, ProductOverlayMessage.parse("""{"kind":"overlayDone","extra":1}"""))
    }

    @Test
    fun `anything else is dropped`() {
        val dropped = listOf(
            "",
            "not json",
            "[]",
            "\"overlayDone\"",
            """{"kind":7}""",
            """{"done":true}""",
            // An overlay page cannot act for the product or open another overlay.
            """{"kind":"action","actionId":"jollity.register"}""",
            """{"kind":"sendOverlay","path":"confetti-overlay.html"}""",
        )

        assertEquals(dropped.map { null }, dropped.map(ProductOverlayMessage::parse))
    }
}
