package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import org.junit.Assert.assertEquals
import org.junit.Test

private const val REJECTED = "<rejected>"
private const val PNG = "data:image/png;base64,iVBORw0KGgo="

class ChatRoomIconTest {
    // A product on the container bridge registers its room without the core, so its icon must meet the
    // rules the core holds every other product to. The cases come from the core's `validate_chat_icon` tests.
    @Test
    fun `a container product's room icon is held to the core's rules`() {
        val icons = mapOf(
            null to null,
            "  " to null,
            PNG to PNG,
            " $PNG " to PNG,
            "data:image/png,rawbytes" to "data:image/png,rawbytes",
            "https://example.invalid/avatar.png" to "https://example.invalid/avatar.png",
            "javascript:alert(document.cookie)" to REJECTED,
            "file:///etc/passwd" to REJECTED,
            "content://com.host.provider/secret" to REJECTED,
            "data:image/svg+xml,<svg onload=alert(1)>" to REJECTED,
            "http://example.invalid/avatar.png" to REJECTED,
            "java\tscript:alert(1)" to REJECTED,
            "\u0000javascript:alert(1)" to REJECTED,
            "https:/\t/evil.invalid/x" to REJECTED,
            "https://​evil.invalid/x" to REJECTED,
            "https://example.invalid/‮gpj.exe" to REJECTED,
            "https://user:pass@example.invalid/avatar.png" to REJECTED,
            "https://user@example.invalid/avatar.png" to REJECTED,
            "data:image/png;base64," + "A".repeat(64 * 1024) to REJECTED,
        )

        assertEquals(icons, icons.mapValues { (icon, _) -> validateChatIcon(icon).getOrElse { REJECTED } })
    }

    // Percent-encoding grows a non-ASCII path threefold, so an icon inside the budget as it arrives can
    // resolve well past it.
    @Test
    fun `an https icon is measured as resolved`() {
        val icon = "https://icons.invalid/" + "é".repeat(64 * 1024 / 4)

        assertEquals(REJECTED, validateChatIcon(icon).getOrElse { REJECTED })
    }
}
