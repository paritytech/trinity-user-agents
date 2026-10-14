package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResource
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Test

class ChatSummaryRoomPolicyTest {
    private val summary = ChatSummary(
        chatId = ChatId.forExtensionRoom("ProductBot_weather", "berlin"),
        preview = ChatPreview.EmptyChat,
        order = Order.ByTimestamp,
        badge = ChatSummaryBadge.Unread(4),
        timestamp = 0L,
        lastMessageSortOrder = null,
        roomMetadata = RoomMetadata(name = null, icon = null),
        hasUnseenReaction = true,
        customPreviewRenderer = null,
    )

    @Test
    fun `a person chat and a room without a product badge keep the host badge`() {
        assertSame(summary, summary.withRoomPolicy(null))
        assertSame(summary, summary.withRoomPolicy(ChatRoomPolicy.Defaults))
    }

    @Test
    fun `a product count replaces the host count and hides the reaction mark`() {
        val result = summary.withRoomPolicy(policyWithBadge(ChatRoomPolicy.ListBadge.Count(7)))

        assertEquals(ChatSummaryBadge.Unread(7), result.badge)
        assertFalse(result.hasUnseenReaction)
    }

    @Test
    fun `a product count of zero and a hidden badge show no badge`() {
        assertEquals(ChatSummaryBadge.None, summary.withRoomPolicy(policyWithBadge(ChatRoomPolicy.ListBadge.Count(0))).badge)
        assertEquals(ChatSummaryBadge.None, summary.withRoomPolicy(policyWithBadge(ChatRoomPolicy.ListBadge.Hidden)).badge)
    }

    @Test
    fun `a product image becomes the badge`() {
        val image = ChatResource.Preimage(ByteArray(32) { 7 }.toDataByteArray())

        assertEquals(ChatSummaryBadge.Image(image), summary.withRoomPolicy(policyWithBadge(ChatRoomPolicy.ListBadge.Image(image))).badge)
    }

    private fun policyWithBadge(badge: ChatRoomPolicy.ListBadge) = ChatRoomPolicy.Defaults.copy(listBadge = badge)
}
