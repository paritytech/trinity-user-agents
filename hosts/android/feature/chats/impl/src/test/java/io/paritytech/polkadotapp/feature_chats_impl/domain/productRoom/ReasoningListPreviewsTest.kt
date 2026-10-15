package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.extension.RoomMetadata
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Order
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageIds
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatReasoningMessages
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class ReasoningListPreviewsTest {
    private val room = ChatId.forExtensionRoom("ProductBot_sample.dot", "host")
    private val answer = message("answer", timestamp = 1)
    private val reasoning = message("thinking", timestamp = 2)
    private val reasoningIds = MutableStateFlow<Map<ChatId, Set<String>>>(emptyMap())
    private var reads = 0
    private val previews = ReasoningListPreviews(
        reasoningMessages = ChatReasoningMessages { reasoningIds },
        roomMessages = RoomMessagesReader {
            reads++
            listOf(reasoning, answer)
        },
    )

    @Test
    fun `a reasoning row is never the preview of its room`() = runBlocking<Unit> {
        reasoningIds.value = mapOf(room to setOf("thinking"))

        val shown = previews.apply(flowOf(listOf(summary(reasoning)))).first().single()

        assertEquals(ChatPreview.Message(answer, Order.ByTimestamp), shown.preview)
    }

    @Test
    fun `another last message stays the preview`() = runBlocking<Unit> {
        reasoningIds.value = mapOf(room to setOf("thinking"))

        val shown = previews.apply(flowOf(listOf(summary(answer)))).first().single()

        assertEquals(ChatPreview.Message(answer), shown.preview)
        assertEquals(0, reads)
    }

    @Test
    fun `the edits of a reasoning row read the room once`() = runBlocking<Unit> {
        reasoningIds.value = mapOf(room to setOf("thinking"))

        repeat(3) { previews.apply(flowOf(listOf(summary(reasoning)))).first() }

        assertEquals(1, reads)
    }

    private fun message(productId: String, timestamp: Long) = ChatMessage(
        id = ProductMessageIds.stored(productId, room),
        chatId = room,
        timestamp = timestamp,
        origin = ChatMessageOrigin.Extension("ProductBot_sample.dot"),
        content = ChatMessage.Content.Text(productId),
        status = ChatMessage.Status.IS_READ,
    )

    private fun summary(last: ChatMessage) = ChatSummary(
        chatId = room,
        preview = ChatPreview.Message(last),
        order = Order.ByTimestamp,
        badge = ChatSummaryBadge.None,
        timestamp = last.timestamp,
        lastMessageSortOrder = last.timestamp,
        roomMetadata = RoomMetadata(name = "Sample", icon = null),
        hasUnseenReaction = false,
        customPreviewRenderer = null,
    )
}
