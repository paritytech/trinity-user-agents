package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.message.ProductsMessageContent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatReaction
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.HostRejection
import uniffi.truapi.NativeChatFeedRejection
import uniffi.truapi.ChatMessage as CoreChatMessage

private const val ROOM = "lobby"

class ProductChatHostBridgeFeedTest {
    private val productId = ProductId.fromStoredValue("game.dot")
    private val room = ProductChatIdParameter(ROOM)
    private val messaging: ProductChatMessaging = mockk()
    private val bridge = ProductChatHostBridge(productId, messaging)

    @Test
    fun `a stored new message keeps its product id, participant, time and status`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)

        bridge.updateFeed(ROOM, ChatFeedUpdate.New(coreMessage(ChatMessageBody.Text("hi"))), persist = true)

        val message = (update.captured as ProductFeedUpdateModel.New).message
        assertEquals("m1", message.messageId)
        assertEquals("alice", message.participantId)
        assertEquals(9L, message.timestamp)
        assertEquals(ProductMessageStatusModel.DELIVERED, message.status)
        assertEquals("parent", message.replyTo)
        assertEquals("hi", (message.content as ChatMessage.Content.Text).text)
        assertEquals(1, message.reactions.single().count)
    }

    @Test
    fun `a custom body is drawn by the renderer of the product`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)

        bridge.updateFeed(ROOM, ChatFeedUpdate.Edited("m1", ChatMessageBody.Custom("card", byteArrayOf(4), "A card"), 3uL), persist = true)

        val edited = update.captured as ProductFeedUpdateModel.Edited
        val custom = edited.content as ChatMessage.Content.Custom<*>
        assertEquals(productId.toChatExtensionId(), custom.rendererId)
        assertEquals(ProductsMessageContent("card", byteArrayOf(4).toDataByteArray(), "A card"), custom.content.getOrNull())
        assertEquals(3L, edited.editedAt)
    }

    @Test
    fun `rich text keeps its media items`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)

        bridge.updateFeed(ROOM, ChatFeedUpdate.Edited("m1", ChatMessageBody.RichText("caption", emptyList()), 1uL), persist = true)

        val body = ((update.captured as ProductFeedUpdateModel.Edited).content as ChatMessage.Content.ProductBody).body
        assertEquals(ProductMessageBody.RichText("caption", emptyList()), body)
    }

    @Test
    fun `each refusal of the store reaches the product as its own rejection`() = runTest {
        suspend fun rejectionFor(failure: Throwable): Throwable? {
            coEvery { messaging.applyFeedUpdate(room, any()) } returns Result.failure(failure)
            return runCatching { bridge.updateFeed(ROOM, ChatFeedUpdate.Deleted("m1"), persist = true) }.exceptionOrNull()
        }

        assertTrue(rejectionFor(ProductFeedRejection.UnknownMessage()) is NativeChatFeedRejection.UnknownMessage)
        assertTrue(rejectionFor(ProductFeedRejection.DuplicateMessage()) is NativeChatFeedRejection.DuplicateMessage)
        assertTrue(rejectionFor(ProductFeedRejection.NotOwner()) is NativeChatFeedRejection.NotOwner)
        assertTrue(rejectionFor(ProductFeedRejection.UnknownRoom()) is NativeChatFeedRejection.Rejected)
        assertTrue(rejectionFor(IllegalStateException("disk full")) is NativeChatFeedRejection.Rejected)
    }

    @Test
    fun `a live update goes to the open screens and is never stored`() = runTest {
        coEvery { messaging.deliverLiveFeedUpdate(room, any()) } returns false

        bridge.updateFeed(ROOM, ChatFeedUpdate.StatusChanged("m1", ChatMessageStatus.FAILED), persist = false)

        coVerify { messaging.deliverLiveFeedUpdate(room, ProductFeedUpdateModel.StatusChanged("m1", ProductMessageStatusModel.FAILED)) }
        coVerify(exactly = 0) { messaging.applyFeedUpdate(any(), any()) }
    }

    @Test
    fun `a v1 reaction applies as a reaction of the product and answers the message it reacts to`() = runTest {
        coEvery { messaging.applyFeedUpdate(room, any()) } returns Result.success(Unit)

        val id = bridge.postMessage(ROOM, ChatMessageContent.Reaction(ChatReaction("m1", "👍")), alt = null)
        bridge.postMessage(ROOM, ChatMessageContent.ReactionRemoved(ChatReaction("m1", "👍")), alt = null)

        assertEquals("m1", id)
        coVerify { messaging.applyFeedUpdate(room, ProductFeedUpdateModel.ReactionPlaced("m1", "product", "👍")) }
        coVerify { messaging.applyFeedUpdate(room, ProductFeedUpdateModel.ReactionRemoved("m1", "product", "👍")) }
    }

    @Test
    fun `a v1 reaction on a message the room does not hold is rejected`() = runTest {
        coEvery { messaging.applyFeedUpdate(room, any()) } returns Result.failure(ProductFeedRejection.UnknownMessage())

        val thrown = runCatching {
            bridge.postMessage(ROOM, ChatMessageContent.Reaction(ChatReaction("m9", "👍")), alt = null)
        }.exceptionOrNull()

        assertTrue(thrown is HostRejection)
    }

    @Test
    fun `an update without a room is refused before the store`() = runTest {
        val thrown = runCatching { bridge.updateFeed("", ChatFeedUpdate.Deleted("m1"), persist = true) }.exceptionOrNull()

        assertTrue(thrown is NativeChatFeedRejection.Rejected)
    }

    private fun coreMessage(body: ChatMessageBody) = CoreChatMessage(
        messageId = "m1",
        origin = ChatMessageOrigin("alice"),
        timestamp = 9uL,
        body = body,
        status = ChatMessageStatus.DELIVERED,
        replyTo = "parent",
        editedAt = null,
        reactions = listOf(ChatReactionSummary("👍", 1u, false)),
    )
}
