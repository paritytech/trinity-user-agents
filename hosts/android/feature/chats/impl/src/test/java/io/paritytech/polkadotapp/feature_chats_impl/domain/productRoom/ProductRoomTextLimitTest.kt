package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductRoomTextLimitTest {
    private val productRoom = ChatId.forExtensionRoom("ProductBot_weather", "lobby")

    @Test
    fun `a product room takes a text of up to 16 KiB and refuses a longer one`() {
        assertTrue(productRoom.takesText("x".repeat(PRODUCT_ROOM_TEXT_MAX_BYTES)))
        assertFalse(productRoom.takesText("x".repeat(PRODUCT_ROOM_TEXT_MAX_BYTES + 1)))
    }

    @Test
    fun `the limit counts the bytes of the text, not its characters`() {
        assertFalse(productRoom.takesText("é".repeat(PRODUCT_ROOM_TEXT_MAX_BYTES / 2 + 1)))
    }

    @Test
    fun `a person chat has no limit of the product rooms`() {
        assertTrue(ChatId.fromRawValue(ByteArray(32) { 1 }).takesText("x".repeat(PRODUCT_ROOM_TEXT_MAX_BYTES * 2)))
    }
}
