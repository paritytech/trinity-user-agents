package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductRoomMessage
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

/** Feed updates for a product-source room that no screen shows change its cached page (gap 13 of the parity report). */
class ProductMessageCacheTest {
    private val room = ChatId.forExtensionRoom("ProductBot_chat.dot", "dm-1")

    @Test
    fun `each feed update changes the cached page`() = runBlocking<Unit> {
        val cache = ProductMessageCache()
        cache.store(room, entry(listOf(message("m1", 1), message("m2", 2), message("m3", 3))))

        cache.apply(room, ProductFeedUpdateModel.New(message("m5", 5)))
        cache.apply(room, ProductFeedUpdateModel.New(message("m4", 4)))
        val changed = ChatMessage.Content.Text("changed")
        cache.apply(room, ProductFeedUpdateModel.Edited("m1", changed, 9))
        cache.apply(room, ProductFeedUpdateModel.Replaced(message("m2", 2).copy(content = ChatMessage.Content.Text("replaced"))))
        cache.apply(room, ProductFeedUpdateModel.ReactionPlaced("m3", "c1", "👍"))
        cache.apply(room, ProductFeedUpdateModel.StatusChanged("m5", ProductMessageStatusModel.READ))

        val cached = cache.entry(room)!!
        assertEquals(listOf("m1", "m2", "m3", "m4", "m5"), cached.messages.map { it.messageId })
        assertSame(changed, cached.messages[0].content)
        assertEquals(9L, cached.messages[0].editedAt)
        assertEquals("replaced", (cached.messages[1].content as ChatMessage.Content.Text).text)
        assertEquals(null, cached.messages[1].editedAt)
        assertEquals(listOf(ProductReactionSummaryModel("👍", 1, false)), cached.messages[2].reactions)
        assertEquals(ProductMessageStatusModel.READ, cached.messages[4].status)
        assertEquals("0", cached.next)
    }

    @Test
    fun `a full page drops its oldest message and its cursor`() = runBlocking<Unit> {
        val cache = ProductMessageCache(capacity = 4, messageLimit = 2)
        cache.store(room, entry(listOf(message("m1", 1), message("m2", 2))))

        cache.apply(room, ProductFeedUpdateModel.New(message("m3", 3)))

        val cached = cache.entry(room)!!
        assertEquals(listOf("m2", "m3"), cached.messages.map { it.messageId })
        assertNull(cached.next)
    }

    @Test
    fun `a room with no cached page gets none`() = runBlocking<Unit> {
        val cache = ProductMessageCache()

        cache.apply(room, ProductFeedUpdateModel.New(message("m1", 1)))

        assertNull(cache.entry(room))
    }

    private fun entry(messages: List<ProductRoomMessage>) = ProductRoomCacheEntry(
        messages = messages,
        next = "0",
        seenMessageId = messages.lastOrNull()?.messageId,
        seenTimestamp = messages.lastOrNull()?.timestamp,
        reportedSeenMessageId = null,
    )

    companion object {
        fun message(id: String, timestamp: Long) = ProductRoomMessage(
            messageId = id,
            participantId = "c1",
            timestamp = timestamp,
            content = ChatMessage.Content.Text("message $id"),
            status = ProductMessageStatusModel.DELIVERED,
            replyTo = null,
            editedAt = null,
            reactions = emptyList(),
            request = false,
        )
    }
}
