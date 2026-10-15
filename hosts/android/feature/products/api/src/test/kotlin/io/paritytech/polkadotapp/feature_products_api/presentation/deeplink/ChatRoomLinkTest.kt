package io.paritytech.polkadotapp.feature_products_api.presentation.deeplink

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ChatRoomLinkTest {
    @Test
    fun `the room of a chat room link is its third segment, trimmed`() {
        assertEquals("lobby", chatRoomIdOf(listOf("-", "chat", "lobby")))
        assertEquals("room with spaces", chatRoomIdOf(listOf("-", "chat", " room with spaces ")))
    }

    @Test
    fun `another path is no chat room link`() {
        assertNull(chatRoomIdOf(listOf("-", "chat")))
        assertNull(chatRoomIdOf(listOf("-", "chat", " ")))
        assertNull(chatRoomIdOf(listOf("-", "chat", "lobby", "extra")))
        assertNull(chatRoomIdOf(listOf("-", "pocket", "open")))
        assertNull(chatRoomIdOf(listOf("chat", "lobby")))
        assertNull(chatRoomIdOf(emptyList()))
    }
}
