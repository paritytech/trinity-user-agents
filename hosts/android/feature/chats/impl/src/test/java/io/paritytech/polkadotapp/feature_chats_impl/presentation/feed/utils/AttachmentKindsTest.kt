package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils

import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import org.junit.Assert.assertEquals
import org.junit.Test

class AttachmentKindsTest {
    @Test
    fun `a person chat takes photos and videos`() {
        assertEquals(setOf(AttachmentKind.PHOTO, AttachmentKind.VIDEO), (null as ChatRoomPolicy?).attachmentKinds())
    }

    @Test
    fun `a product room takes the kinds of its input, and none without an input`() {
        val fileOnly = ChatRoomPolicy.DefaultInput.copy(allowsPhoto = false, allowsVideo = false, allowsFile = true)

        assertEquals(setOf(AttachmentKind.PHOTO, AttachmentKind.VIDEO), ChatRoomPolicy.Defaults.attachmentKinds())
        assertEquals(setOf(AttachmentKind.FILE), ChatRoomPolicy.Defaults.copy(footer = ChatRoomPolicy.Footer.TextInput(fileOnly)).attachmentKinds())
        assertEquals(emptySet<AttachmentKind>(), ChatRoomPolicy.Defaults.copy(footer = ChatRoomPolicy.Footer.Empty).attachmentKinds())
    }
}
