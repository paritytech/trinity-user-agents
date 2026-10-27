package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomBadgeModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.productMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy.ListBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy.ListPreview
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatRoomPolicyTest {
    @Test
    fun `a room without declarations gets every host default`() {
        val policy = ChatRoomPolicy.from(null)

        assertEquals(ChatRoomPolicy.Defaults, policy)
        assertEquals(ChatRoomPolicy.MessageStore.HOST, policy.messageStore)
        assertNull(policy.listPreview)
        assertEquals(ListBadge.HostUnreadCount, policy.listBadge)
        assertNull(policy.listTimestamp)
        assertFalse(policy.reportsSeen)
    }

    @Test
    fun `the declared configuration of the host default gives the defaults`() {
        assertEquals(ChatRoomPolicy.Defaults.copy(configGeneration = 1), ChatRoomPolicy.from(declarations()))
    }

    @Test
    fun `empty declarations give the defaults`() {
        assertEquals(ChatRoomPolicy.Defaults, ChatRoomPolicy.from(ChatRoomDeclarations.EMPTY))
        assertEquals(ChatRoomPolicy.Defaults.copy(configGeneration = 1), ChatRoomPolicy.from(declarations(config = null)))
    }

    @Test
    fun `the policy carries the configuration generation, so a screen asks its questions again`() {
        assertEquals(7L, ChatRoomPolicy.from(declarations().copy(configGeneration = 7)).configGeneration)
    }

    @Test
    fun `the product source and the seen reports come from the configuration`() {
        val config = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true, reportSeen = true)

        val policy = ChatRoomPolicy.from(declarations(config = config))

        assertEquals(ChatRoomPolicy.MessageStore.PRODUCT, policy.messageStore)
        assertTrue(policy.reportsSeen)
    }

    @Test
    fun `the preview sets the last message, the reaction, the timestamp and the badge`() {
        val last = productMessage(1)
        val reaction = ChatPreviewReactionModel(emoji = "👍", byUser = false, message = last, removed = false)
        val policy = ChatRoomPolicy.from(
            declarations(
                preview = preview(
                    timestamp = 5_000L,
                    badge = ChatRoomBadgeModel.Count(3),
                    lastMessage = last,
                    lastReaction = reaction,
                    unseenReactions = 2,
                )
            )
        )

        assertEquals(ListPreview(last, reaction, unseenReactions = 2), policy.listPreview)
        assertEquals(5_000L, policy.listTimestamp)
        assertEquals(ListBadge.Count(3), policy.listBadge)
    }

    @Test
    fun `a preview without a badge keeps the host count and a zero timestamp reads as none`() {
        val policy = ChatRoomPolicy.from(declarations(preview = preview(timestamp = 0L, lastMessage = productMessage(1))))

        assertEquals(ListBadge.HostUnreadCount, policy.listBadge)
        assertNull(policy.listTimestamp)
    }

    @Test
    fun `a hidden badge reads as hidden`() {
        val hidden = ChatRoomPolicy.from(declarations(preview = preview(badge = ChatRoomBadgeModel.Hidden)))

        assertEquals(ListBadge.Hidden, hidden.listBadge)
    }

    @Test
    fun `a negative count reads as zero`() {
        val policy = ChatRoomPolicy.from(declarations(preview = preview(badge = ChatRoomBadgeModel.Count(-4))))

        assertEquals(ListBadge.Count(0), policy.listBadge)
    }

    @Test
    fun `the metadata gives the description`() {
        val policy = ChatRoomPolicy.from(declarations(description = "Weather for your city"))

        assertEquals("Weather for your city", policy.description)
    }
}
