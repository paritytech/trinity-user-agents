package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import org.junit.Assert.assertEquals
import org.junit.Test

private const val REJECTED = "<rejected>"
private const val PNG = "data:image/png;base64,iVBORw0KGgo="

class ProductChatIconTest {
    // The core's own rules decide, so a container product's icon is held to what a TrUAPI product's already is.
    // What reaches the room is the core's screened form, and no icon at all is absence rather than an empty icon.
    @Test
    fun `a room icon is what the core screens it to`() {
        val icons = mapOf(
            null to null,
            "  " to null,
            " $PNG " to PNG,
            "https://example.invalid" to "https://example.invalid/",
            "data:image/svg+xml,<svg onload=alert(1)>" to REJECTED,
            "javascript:alert(1)" to REJECTED,
            "data:image/png;base64," + "A".repeat(64 * 1024) to REJECTED,
        )

        assertEquals(icons, icons.mapValues { (icon, _) -> ProductChatIcon.fromString(icon).map { it?.value }.getOrElse { REJECTED } })
    }
}
