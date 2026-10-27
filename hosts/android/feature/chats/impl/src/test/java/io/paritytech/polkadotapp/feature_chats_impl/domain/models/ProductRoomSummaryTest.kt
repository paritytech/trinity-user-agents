package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatPreviewReactionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductRoomSummaryTest {
    private val room = ChatId.forExtensionRoom("ProductBot_weather", "berlin")
    private val summary = ChatSummary(
        chatId = room,
        preview = ChatPreview.EmptyChat,
        order = Order.ByTimestamp,
        badge = ChatSummaryBadge.Unread(4),
        timestamp = 10L,
        lastMessageSortOrder = null,
        roomMetadata = RoomMetadata(name = null, icon = null),
        hasUnseenReaction = true,
        customPreviewRenderer = null,
    )
    private val productStore = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)

    @Test
    fun `a person chat keeps the host row`() {
        assertSame(summary, summary.withRoomPolicy(null))
    }

    @Test
    fun `a product count replaces the host count, and a room that the product keeps counts nothing itself`() {
        assertEquals(ChatSummaryBadge.Unread(7), summary.withRoomPolicy(productStore.copy(listBadge = ChatRoomPolicy.ListBadge.Count(7))).badge)
        assertEquals(ChatSummaryBadge.None, summary.withRoomPolicy(productStore.copy(listBadge = ChatRoomPolicy.ListBadge.Count(0))).badge)
        assertEquals(ChatSummaryBadge.None, summary.withRoomPolicy(productStore.copy(listBadge = ChatRoomPolicy.ListBadge.Hidden)).badge)

        val hostCount = summary.withRoomPolicy(productStore)
        assertEquals(ChatSummaryBadge.None, hostCount.badge)
        assertFalse(hostCount.hasUnseenReaction)
    }

    @Test
    fun `the last message of the product is the preview and its time orders the row`() {
        val policy = productStore.copy(
            listTimestamp = 2_000L,
            listPreview = ChatRoomPolicy.ListPreview(lastMessage = message("m1", ProductParticipant.USER, "hi"), lastReaction = null, unseenReactions = 0),
        )

        val row = summary.withRoomPolicy(policy)

        val preview = (row.preview as ChatPreview.Message).message
        assertSame(ChatMessageOrigin.User, preview.origin)
        assertEquals("hi", (preview.content as ChatMessage.Content.Text).text)
        assertEquals(2_000L, row.timestamp)
        assertFalse(row.hasUnseenReaction)
    }

    @Test
    fun `a newer reaction is the preview, and an unseen one shows the heart`() {
        val reacted = message("m1", ProductParticipant.USER, "hi")
        val policy = productStore.copy(
            listPreview = ChatRoomPolicy.ListPreview(
                lastMessage = reacted,
                lastReaction = ChatPreviewReactionModel(emoji = "👍", byUser = false, message = reacted, removed = false),
                unseenReactions = 1,
            ),
        )

        val row = summary.withRoomPolicy(policy)

        val preview = (row.preview as ChatPreview.Message).message
        val content = preview.content as ChatMessage.Content.Reacted
        assertEquals("👍", content.content.emoji)
        assertEquals("m1", content.messageId)
        assertTrue(preview.origin is ChatMessageOrigin.Extension)
        assertTrue(row.hasUnseenReaction)
    }

    @Test
    fun `a removed reaction is the removed reaction line`() {
        val reacted = message("m1", ProductParticipant.USER, "hi")
        val policy = productStore.copy(
            listPreview = ChatRoomPolicy.ListPreview(
                lastMessage = reacted,
                lastReaction = ChatPreviewReactionModel(emoji = "👍", byUser = false, message = reacted, removed = true),
                unseenReactions = 0,
            ),
        )

        val preview = (summary.withRoomPolicy(policy).preview as ChatPreview.Message).message

        assertEquals("m1", (preview.content as ChatMessage.Content.ReactionRemoved).messageId)
    }

    private fun message(id: String, participant: String, text: String) = ProductRoomMessage(
        messageId = id,
        participantId = participant,
        timestamp = 1_000L,
        content = ChatMessage.Content.Text(text),
        status = ProductMessageStatusModel.READ,
        replyTo = null,
        editedAt = null,
        reactions = emptyList(),
        request = false,
    )
}
