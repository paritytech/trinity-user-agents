package io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay

import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

private const val PNG_ICON = "data:image/png;base64,iVBORw=="

class ChatDisplayGeneratorTest {
    private val generator = ChatDisplayGenerator(mockk())
    private val manifestIconDisplay = ChatDisplay(
        name = "Jollity",
        avatar = ChatAvatar.Url("product-icon:dim2.paseo", placeholder = ChatAvatar.Account("Jollity", byteArrayOf(1))),
    )

    // The worker's own icon for its room wins over the one its manifest declares.
    @Test
    fun `a room draws the inline icon its worker registered`() {
        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = PNG_ICON), manifestIconDisplay)

        assertEquals(PNG_ICON, (display.avatar as ChatAvatar.Url).url)
    }

    // The core admits an inline icon by its media type alone, so bytes that fail to decode must leave the
    // room's letter rather than an empty circle.
    @Test
    fun `an inline room icon is drawn over the room's letter`() {
        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = PNG_ICON), manifestIconDisplay)

        assertSame((manifestIconDisplay.avatar as ChatAvatar.Url).placeholder, (display.avatar as ChatAvatar.Url).placeholder)
    }

    // A chat with no manifest icon defaults to its letter, which an inline icon keeps beneath it.
    @Test
    fun `an inline room icon over a letter default keeps that letter`() {
        val letter = ChatAvatar.Account("Jollity", byteArrayOf(1))

        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = PNG_ICON), ChatDisplay("Jollity", letter))

        assertSame(letter, (display.avatar as ChatAvatar.Url).placeholder)
    }

    // Drawing a remote room icon would have the app fetch whatever URL the product chose.
    @Test
    fun `a room with a remote icon shows the manifest icon`() {
        val roomMetadata = RoomMetadata(name = null, icon = "https://example.com/icon.png")

        assertSame(manifestIconDisplay.avatar, generator.applyRoomMetadata(roomMetadata, manifestIconDisplay).avatar)
    }
}
