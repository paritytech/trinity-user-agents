package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
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

    @Test
    fun `a room that its product pins goes above the chats that are not pinned`() {
        val pinnedByProduct = chatWithMessage(seed = 1, sortOrder = 1, timestamp = now)
        val latest = chatWithMessage(seed = 2, sortOrder = 5, timestamp = now + 1_000)

        val ordered = chatListOrder(listOf(latest, pinnedByProduct), mapOf(pinnedByProduct.chatId to listOrder(isPinned = true)))

        assertEquals(listOf(pinnedByProduct, latest), ordered)
    }

    @Test
    fun `a product timestamp goes in before the first chat whose last message is older`() {
        val newer = chatWithMessage(seed = 1, sortOrder = 9, timestamp = now + 2_000)
        val older = chatWithMessage(seed = 2, sortOrder = 8, timestamp = now)
        val product = chatWithMessage(seed = 3, sortOrder = 1, timestamp = now - 60_000)

        val ordered = chatListOrder(listOf(older, product, newer), mapOf(product.chatId to listOrder(timestamp = now + 1_000)))

        assertEquals(listOf(newer, product, older), ordered)
    }

    @Test
    fun `the chats that sort by their last message keep their order around the product rooms`() {
        val first = chatWithMessage(seed = 1, sortOrder = 3, timestamp = now)
        val second = chatWithMessage(seed = 2, sortOrder = 2, timestamp = now + 5_000)
        val third = chatWithMessage(seed = 3, sortOrder = 1, timestamp = now - 5_000)
        val product = emptyChat(seed = 4, createdAt = now)

        val ordered = chatListOrder(listOf(third, product, second, first), mapOf(product.chatId to listOrder(timestamp = now - 1_000)))

        assertEquals(listOf(first, second, product, third), ordered)
    }

    @Test
    fun `product rooms sort by their timestamps and go above chats without messages`() {
        val empty = emptyChat(seed = 1, createdAt = now + 60_000)
        val oldProduct = emptyChat(seed = 2, createdAt = now)
        val newProduct = emptyChat(seed = 3, createdAt = now)

        val ordered = chatListOrder(
            listOf(empty, oldProduct, newProduct),
            mapOf(
                oldProduct.chatId to listOrder(timestamp = now - 1_000),
                newProduct.chatId to listOrder(timestamp = now + 1_000),
            ),
        )

        assertEquals(listOf(newProduct, oldProduct, empty), ordered)
    }

    @Test
    fun `pinned rooms sort by recency among themselves`() {
        val pinnedOld = chatWithMessage(seed = 1, sortOrder = 1, timestamp = now, order = Order.PinToTop)
        val pinnedProduct = emptyChat(seed = 2, createdAt = now)
        val unpinned = chatWithMessage(seed = 3, sortOrder = 9, timestamp = now + 9_000)

        val ordered = chatListOrder(
            listOf(unpinned, pinnedOld, pinnedProduct),
            mapOf(pinnedProduct.chatId to listOrder(isPinned = true, timestamp = now + 1_000)),
        )

        assertEquals(listOf(pinnedProduct, pinnedOld, unpinned), ordered)
    }

    @Test
    fun `without product orders the list keeps the order of the last messages`() {
        val alice = chatWithMessage(seed = 1, sortOrder = 1, timestamp = now + 60_000)
        val bob = chatWithMessage(seed = 2, sortOrder = 2, timestamp = now)
        val pinned = emptyChat(seed = 3, createdAt = now, order = Order.PinToTop)

        val summaries = listOf(alice, pinned, bob)

        assertEquals(summaries.sortedWith(chatSummaryOrder), chatListOrder(summaries, emptyMap()))
    }

    private fun listOrder(isPinned: Boolean = false, timestamp: Long? = null) = ChatRoomPolicy.ListOrder(isPinned, timestamp)

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
