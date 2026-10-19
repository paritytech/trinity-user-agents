package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatMessage
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatPageAnchor
import uniffi.truapi.ChatReactionSummary

// The room that the mock product keeps answers `messages_query` and `search_query` as a product must, so the window of
// the room screen fills without gaps or repeats.
class MockChatConversationTest {
    private fun conversation() = MockChatConversation(
        (1..7).map { number ->
            ChatMessage(
                messageId = "m$number",
                origin = ChatMessageOrigin("product"),
                timestamp = number.toULong(),
                body = ChatMessageBody.Text(if (number % 2 == 0) "even $number" else "odd $number"),
                status = ChatMessageStatus.SENT,
                replyTo = null,
                editedAt = null,
                reactions = emptyList(),
            )
        },
    )

    @Test
    fun `pages backwards cover the room once and end`() {
        val conversation = conversation()

        val newest = conversation.page(ChatPageAnchor.Before(null), size = 3)
        val older = conversation.page(ChatPageAnchor.Before(newest.next), size = 3)
        val oldest = conversation.page(ChatPageAnchor.Before(older.next), size = 3)

        assertEquals(listOf("m5", "m6", "m7"), newest.messages.map { it.messageId })
        assertEquals(listOf("m2", "m3", "m4"), older.messages.map { it.messageId })
        assertEquals(listOf("m1"), oldest.messages.map { it.messageId })
        assertNull(oldest.next)
    }

    @Test
    fun `pages forwards start at the oldest and end`() {
        val conversation = conversation()

        val first = conversation.page(ChatPageAnchor.After(null), size = 4)
        val second = conversation.page(ChatPageAnchor.After(first.next), size = 4)

        assertEquals(listOf("m1", "m2", "m3", "m4"), first.messages.map { it.messageId })
        assertEquals(listOf("m5", "m6", "m7"), second.messages.map { it.messageId })
        assertNull(second.next)
    }

    @Test
    fun `search gives the newest hits first and pages`() {
        val conversation = conversation()

        val first = conversation.search("EVEN", cursor = null, size = 2)
        val second = conversation.search("even", cursor = first.next, size = 2)

        assertEquals(listOf("m6", "m4"), first.hits.map { it.messageId })
        assertEquals(listOf("m2"), second.hits.map { it.messageId })
        assertNull(second.next)
        assertTrue(conversation.search("  ", cursor = null, size = 2).hits.isEmpty())
    }

    // A second reaction of the user with the same emoji must not count twice, and removing the last one removes the
    // summary, or the room would draw a chip with a count of zero.
    @Test
    fun `the user reacts once and the empty summary goes`() {
        val conversation = conversation()

        conversation.react("m1", "👍", placed = true, byUser = true)
        conversation.react("m1", "👍", placed = true, byUser = true)
        conversation.react("m1", "👍", placed = true, byUser = false)
        assertEquals(listOf(ChatReactionSummary("👍", 2u, reactedByUser = true)), conversation.message("m1")?.reactions)

        conversation.react("m1", "👍", placed = false, byUser = true)
        assertEquals(listOf(ChatReactionSummary("👍", 1u, reactedByUser = false)), conversation.message("m1")?.reactions)

        conversation.react("m1", "👍", placed = false, byUser = false)
        assertEquals(emptyList<ChatReactionSummary>(), conversation.message("m1")?.reactions)
    }

    @Test
    fun `an action on a message that is gone is refused`() {
        val conversation = conversation()

        assertTrue(conversation.remove("m3"))
        assertFalse(conversation.edit("m3", ChatMessageBody.Text("late"), editedAt = 9u))
        assertFalse(conversation.setStatus("m3", ChatMessageStatus.READ))
    }
}
