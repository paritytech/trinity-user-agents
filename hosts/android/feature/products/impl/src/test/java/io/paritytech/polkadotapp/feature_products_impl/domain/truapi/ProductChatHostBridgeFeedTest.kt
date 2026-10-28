package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
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
import uniffi.truapi.ChatCallRecord
import uniffi.truapi.ChatContactEvent
import uniffi.truapi.ChatCustomMessage
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMediaItem
import uniffi.truapi.ChatMediaMeta
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatPaymentRecord
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.ChatReplyPreview
import uniffi.truapi.ChatResource
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
    fun `a welcome text of a chat request draws as a request message`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)

        bridge.updateFeed(ROOM, ChatFeedUpdate.New(coreMessage(ChatMessageBody.Text("hi"), request = true)), persist = true)

        val message = (update.captured as ProductFeedUpdateModel.New).message
        assertTrue(message.request)
        val welcome = requireNotNull((message.content as ChatMessage.Content.ChatRequest).welcome)
        assertEquals("hi", welcome.text)
    }

    // The events, the calls and the payments of a contact room come with the Chat Extension v2 additions.
    @Test
    fun `contact events, calls and payments draw as unsupported messages`() {
        assertTrue(ChatMessageBody.Event(ChatContactEvent.CONTACT_ADDED).toContent(productId) is ChatMessage.Content.Unsupported)
        assertTrue(ChatMessageBody.Call(ChatCallRecord(video = true, answeredAt = null, closed = null)).toContent(productId) is ChatMessage.Content.Unsupported)
        assertTrue(ChatMessageBody.Payment(ChatPaymentRecord(amount = "1000", legacy = false)).toContent(productId) is ChatMessage.Content.Unsupported)
    }

    @Test
    fun `rich text with media keeps its text and its media as a product body`() {
        val hop = ChatResource.Hop(identifier = ByteArray(32) { 1 }, ticket = ByteArray(32) { 2 }, node = "wss://hop.example", size = 12uL)
        val image = ChatMediaItem.Image(hop, ChatMediaMeta("image/png", 12uL, 2u, 1u, placeholder = null, alt = "a dot", durationMs = null))

        val content = ChatMessageBody.RichText("Look", listOf(image)).toContent(productId)

        val body = (content as ChatMessage.Content.ProductBody).body as ProductMessageBody.RichText
        assertEquals("Look", body.text)
        val item = body.media.single() as ChatMediaItemModel.Image
        assertEquals("a dot", item.meta.alt)
        assertEquals("wss://hop.example", (item.resource as ChatResourceModel.Hop).node)
    }

    // The renderer of the product draws a custom body, as it draws a custom message that the product posts.
    @Test
    fun `a custom body is stored for the renderer of the product`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)
        val body = ChatMessageBody.Custom(ChatCustomMessage("poll", byteArrayOf(1, 2)), alt = "A poll")

        bridge.updateFeed(ROOM, ChatFeedUpdate.Edited("m1", body, 1uL), persist = true)

        val expected = ChatMessage.Content.Custom(
            rendererId = productId.toChatExtensionId(),
            content = Result.success(ProductsMessageContent("poll", byteArrayOf(1, 2).toDataByteArray(), "A poll")),
        )
        assertEquals(expected, (update.captured as ProductFeedUpdateModel.Edited).content)
    }

    @Test
    fun `rich text without media draws as a text message`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)

        bridge.updateFeed(ROOM, ChatFeedUpdate.Edited("m1", ChatMessageBody.RichText("caption", emptyList()), 1uL), persist = true)

        val content = (update.captured as ProductFeedUpdateModel.Edited).content
        assertEquals("caption", (content as ChatMessage.Content.Text).text)
    }

    @Test
    fun `each refusal of the store reaches the product as its own rejection`() = runTest {
        suspend fun rejectionFor(failure: Throwable): Throwable? {
            coEvery { messaging.applyFeedUpdate(room, any()) } returns Result.failure(failure)
            return runCatching { bridge.updateFeed(ROOM, ChatFeedUpdate.Replaced(coreMessage(ChatMessageBody.Text("hi"))), persist = true) }.exceptionOrNull()
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
    fun `an update without a room is refused before the store`() = runTest {
        val thrown = runCatching { bridge.updateFeed("", ChatFeedUpdate.StatusChanged("m1", ChatMessageStatus.READ), persist = true) }.exceptionOrNull()

        assertTrue(thrown is NativeChatFeedRejection.Rejected)
    }

    @Test
    fun `a reply carries the quote of its replied message`() {
        val reply = coreMessage(ChatMessageBody.Text("yes"))
            .copy(replyPreview = ChatReplyPreview(ChatMessageOrigin("contact"), ChatMessageBody.Text("lunch?")))

        val preview = requireNotNull(reply.toModel(productId).replyPreview)

        assertEquals("contact", preview.participantId)
        assertEquals("lunch?", (preview.content as ChatMessage.Content.Text).text)
    }

    private fun coreMessage(body: ChatMessageBody, request: Boolean = false) = CoreChatMessage(
        messageId = "m1",
        origin = ChatMessageOrigin("alice"),
        timestamp = 9uL,
        body = body,
        status = ChatMessageStatus.DELIVERED,
        replyTo = "parent",
        editedAt = null,
        reactions = listOf(ChatReactionSummary("👍", 1u, false, unseen = false)),
        request = request,
        replyPreview = null,
    )
}
