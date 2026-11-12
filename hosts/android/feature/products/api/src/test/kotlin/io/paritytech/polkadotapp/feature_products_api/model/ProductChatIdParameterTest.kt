package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

private const val EXTENSION = "ProductBot_coinflip.dot"
private const val OTHER_EXTENSION = "ProductBot_other.dot"
private const val ROOM = "lobby"

class ProductChatIdParameterTest {
    @Test
    fun `a room of this extension resolves to its room id`() {
        assertEquals(ProductChatIdParameter(ROOM), ChatId.forExtensionRoom(EXTENSION, ROOM).productRoomId(EXTENSION))
    }

    @Test
    fun `a room of another extension is not ours`() {
        assertNull(ChatId.forExtensionRoom(OTHER_EXTENSION, ROOM).productRoomId(EXTENSION))
    }

    @Test
    fun `the roomless extension chat has no room id`() {
        assertNull(ChatId.forExtension(EXTENSION).productRoomId(EXTENSION))
    }

    @Test
    fun `an empty room id is refused, since the core rejects it`() {
        assertNull(ChatId.forExtensionRoom(EXTENSION, "").productRoomId(EXTENSION))
    }

    @Test
    fun `a contact chat has no room id`() {
        assertNull(ChatId.fromContact(AccountId(ByteArray(32))).productRoomId(EXTENSION))
    }
}
