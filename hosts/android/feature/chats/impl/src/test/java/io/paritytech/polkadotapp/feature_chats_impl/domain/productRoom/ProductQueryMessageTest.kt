package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReaction
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageReactionContent
import io.paritytech.polkadotapp.feature_chats_api.domain.model.MessageRevision
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductReactionSummaryModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import org.junit.Assert.assertEquals
import org.junit.Test

class ProductQueryMessageTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "lobby")

    @Test
    fun `a message of the user carries its latest edit, its status and the reactions`() {
        val message = message("user-1", ChatMessageOrigin.User, participantId = null).copy(replyToMessageId = "product-1")
        val edited = MessageRevision(messageId = "user-1", content = ChatMessage.Content.Text("edited"), chatId = roomId, timestamp = 2_000L)
        val snapshot = snapshot(
            message,
            reactions = mapOf(
                "user-1" to listOf(reaction("user-1", "👍", ChatMessageOrigin.User), reaction("user-1", "👍", ChatMessageOrigin.Extension("x"))),
            ),
            revisions = mapOf("user-1" to edited),
            statuses = mapOf("user-1" to ProductMessageStatusModel.DELIVERED),
        )

        val query = snapshot.productQueryMessage(message)

        assertEquals("user", query.participantId)
        assertEquals("edited", (query.content as ChatMessage.Content.Text).text)
        assertEquals(2_000L, query.editedAt)
        assertEquals("product-1", query.replyTo)
        assertEquals(ProductMessageStatusModel.DELIVERED, query.status)
        assertEquals(listOf(ProductReactionSummaryModel("👍", count = 2, reactedByUser = true)), query.reactions)
    }

    @Test
    fun `a message of the product names its participant, or the product`() {
        val fromAlice = message("m-1", ChatMessageOrigin.Extension("ProductBot_weather"), participantId = "alice")
        val fromProduct = message("m-2", ChatMessageOrigin.Extension("ProductBot_weather"), participantId = null)
        val snapshot = snapshot(fromAlice)

        assertEquals("alice", snapshot.productQueryMessage(fromAlice).participantId)
        assertEquals("product", snapshot.productQueryMessage(fromProduct).participantId)
        assertEquals(ProductMessageStatusModel.SENT, snapshot.productQueryMessage(fromProduct).status)
    }

    private fun message(id: String, origin: ChatMessageOrigin, participantId: String?) = ChatMessage(
        id = id,
        chatId = roomId,
        timestamp = 1_000L,
        origin = origin,
        content = ChatMessage.Content.Text("hello"),
        status = ChatMessage.Status.IS_SENT,
        originParticipantId = participantId,
    )

    private fun reaction(messageId: String, emoji: String, origin: ChatMessageOrigin) =
        ChatMessageReaction(messageId, ChatMessageReactionContent(emoji), origin, 1_000L)

    private fun snapshot(
        message: ChatMessage,
        reactions: Map<String, List<ChatMessageReaction>> = emptyMap(),
        revisions: Map<String, MessageRevision> = emptyMap(),
        statuses: Map<String, ProductMessageStatusModel> = emptyMap(),
    ) = ChatFeedSnapshot(
        messages = listOf(message),
        reactions = reactions,
        revisions = revisions,
        productStatuses = statuses,
        paging = ChatPagingState.Complete,
        readiness = ChatRoomReadiness.READY,
    )
}
