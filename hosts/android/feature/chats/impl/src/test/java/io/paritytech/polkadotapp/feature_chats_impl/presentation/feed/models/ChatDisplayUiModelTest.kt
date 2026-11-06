package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatDisplayUiModelTest {
    // A public profile is reread when its chat opens. Other images have no record to reread, so refreshing them would
    // only download the same image again on every open.
    @Test
    fun `an opened chat refreshes only public profile avatars`() {
        val seed = ByteArray(32) { 1 }
        val avatars = listOf(
            ChatAvatar.publicProfile("alice.01", themeSeed = seed),
            ChatAvatar.Url("https://example.com/icon.png", placeholder = ChatAvatar.Account("Jollity", seed)),
        )

        val refreshed = avatars.map { (it.toUi(refresh = true) as AvatarUiModel.Image).refresh }

        assertEquals(listOf(true, false), refreshed)
    }
}
