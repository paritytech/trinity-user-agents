package io.paritytech.polkadotapp.feature_products_impl.domain.bot.model

import org.junit.Assert.assertEquals
import org.junit.Test

private const val REJECTED = "<rejected>"
private const val PNG = "data:image/png;base64,iVBORw0KGgo="
private const val PREIMAGE = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy"

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

    // The core passes the icon of a version 2 room on as the CID of a Bulletin preimage too. A host that refused it
    // would fail a room that the core accepted. The rules of a container product do not change.
    @Test
    fun `a room icon of a chat callback is also the CID of a preimage`() {
        val icons = mapOf(
            "" to null,
            PREIMAGE to PREIMAGE,
            PNG to PNG,
            "https://example.invalid" to "https://example.invalid/",
            "javascript:alert(1)" to REJECTED,
        )

        assertEquals(icons, icons.mapValues { (icon, _) -> ProductChatIcon.fromCallback(icon).map { it?.value }.getOrElse { REJECTED } })
        assertEquals(REJECTED, ProductChatIcon.fromString(PREIMAGE).map { it?.value }.getOrElse { REJECTED })
    }
}
