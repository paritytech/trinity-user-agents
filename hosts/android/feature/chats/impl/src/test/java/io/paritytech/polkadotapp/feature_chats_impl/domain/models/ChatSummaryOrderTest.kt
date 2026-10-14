package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatSummaryOrderTest {
    private val now = 1_000_000L

    @Test
    fun `chat with the later stored message comes first even when the sender clock ran ahead`() {
        val alice = chatWithMessage(seed = 1, sortOrder = 1, timestamp = now + 60_000)
        val bob = chatWithMessage(seed = 2, sortOrder = 2, timestamp = now)

        assertEquals(listOf(bob, alice), listOf(alice, bob).sortedWith(chatSummaryOrder))
    }

    @Test
    fun `pinned chat stays on top`() {
        val pinned = chatWithMessage(seed = 1, sortOrder = 1, timestamp = now, order = Order.PinToTop)
        val latest = chatWithMessage(seed = 2, sortOrder = 5, timestamp = now + 1_000)

        assertEquals(listOf(pinned, latest), listOf(latest, pinned).sortedWith(chatSummaryOrder))
    }

    @Test
    fun `pinned chat without a visible message stays on top`() {
        val pinnedEmpty = emptyChat(seed = 1, createdAt = now, order = Order.PinToTop)
        val latest = chatWithMessage(seed = 2, sortOrder = 5, timestamp = now + 1_000)

        assertEquals(listOf(pinnedEmpty, latest), listOf(latest, pinnedEmpty).sortedWith(chatSummaryOrder))
    }

    @Test
    fun `chat without messages goes below chats with messages`() {
        val empty = emptyChat(seed = 1, createdAt = now + 60_000)
        val withMessage = chatWithMessage(seed = 2, sortOrder = 0, timestamp = now)

        assertEquals(listOf(withMessage, empty), listOf(empty, withMessage).sortedWith(chatSummaryOrder))
    }

    @Test
    fun `chats from before ordering sort by timestamp below ordered ones`() {
        val legacyOld = chatWithMessage(seed = 1, sortOrder = 0, timestamp = now)
        val legacyNew = chatWithMessage(seed = 2, sortOrder = 0, timestamp = now + 1_000)
        val ordered = chatWithMessage(seed = 3, sortOrder = 1, timestamp = now - 1_000)

        assertEquals(
            listOf(ordered, legacyNew, legacyOld),
            listOf(legacyOld, legacyNew, ordered).sortedWith(chatSummaryOrder)
        )
    }

    private fun chatWithMessage(seed: Byte, sortOrder: Long, timestamp: Long, order: Order = Order.ByTimestamp): ChatSummary {
        val chatId = chatId(seed)
        val message = ChatMessage(
            id = "message-$seed",
            chatId = chatId,
            timestamp = timestamp,
            origin = ChatMessageOrigin.User,
            content = ChatMessage.Content.Text("hi"),
            status = ChatMessage.Status.IS_SENT,
        )
        return summary(chatId, ChatPreview.Message(message, order), order, timestamp, sortOrder)
    }

    private fun emptyChat(seed: Byte, createdAt: Long, order: Order = Order.ByTimestamp): ChatSummary {
        return summary(chatId(seed), ChatPreview.EmptyChat, order, createdAt, lastMessageSortOrder = null)
    }

    private fun summary(
        chatId: ChatId,
        preview: ChatPreview,
        order: Order,
        timestamp: Long,
        lastMessageSortOrder: Long?,
    ) = ChatSummary(
        chatId = chatId,
        preview = preview,
        order = order,
        badge = ChatSummaryBadge.None,
        timestamp = timestamp,
        lastMessageSortOrder = lastMessageSortOrder,
        roomMetadata = RoomMetadata(name = null, icon = null),
        hasUnseenReaction = false,
        customPreviewRenderer = null,
    )

    private fun chatId(seed: Byte) = ChatId.fromContact(ByteArray(32) { seed }.toDataByteArray())
}
