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
    fun `a room draws the icon its worker registered`() {
        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = PNG_ICON), manifestIconDisplay)

        assertEquals(PNG_ICON, (display.avatar as ChatAvatar.Url).url)
    }

    // A room icon can name bytes that are not an image, so it must leave the room's letter beneath it rather
    // than an empty circle.
    @Test
    fun `a room icon is drawn over the room's letter`() {
        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = PNG_ICON), manifestIconDisplay)

        assertSame((manifestIconDisplay.avatar as ChatAvatar.Url).placeholder, (display.avatar as ChatAvatar.Url).placeholder)
    }

    // A chat with no manifest icon defaults to its letter, which a room icon keeps beneath it.
    @Test
    fun `a room icon over a letter default keeps that letter`() {
        val letter = ChatAvatar.Account("Jollity", byteArrayOf(1))

        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = PNG_ICON), ChatDisplay("Jollity", letter))

        assertSame(letter, (display.avatar as ChatAvatar.Url).placeholder)
    }

    // A contact's profile photo replaces the request-provided avatar and keeps the letter beneath it.
    @Test
    fun `a profile photo is drawn over the contact's letter`() {
        val letter = ChatAvatar.Account("alice", byteArrayOf(1))
        val display = generator.applyProfileAvatar("content://avatars/alice-bafkcid.webp", ChatDisplay("alice", letter))
        assertEquals("content://avatars/alice-bafkcid.webp", (display.avatar as ChatAvatar.Url).url)
        assertSame(letter, (display.avatar as ChatAvatar.Url).placeholder)
    }

    // Which room icons are acceptable is decided where a product registers them, so the chat draws any it is given.
    @Test
    fun `a room draws a remote icon too`() {
        val remoteIcon = "https://example.com/icon.png"

        val display = generator.applyRoomMetadata(RoomMetadata(name = null, icon = remoteIcon), manifestIconDisplay)

        assertEquals(remoteIcon, (display.avatar as ChatAvatar.Url).url)
    }
}
