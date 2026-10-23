package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedRejection
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedUpdateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatCallClose
import uniffi.truapi.ChatCallRecord
import uniffi.truapi.ChatContactEvent
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageBody
import uniffi.truapi.ChatMessageOrigin
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatPaymentRecord
import uniffi.truapi.ChatReactionSummary
import uniffi.truapi.ChatReplyPreview
import uniffi.truapi.ChatUserAction
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
    fun `a welcome text of a chat request draws as the native request message`() = runTest {
        val update = slot<ProductFeedUpdateModel>()
        coEvery { messaging.applyFeedUpdate(room, capture(update)) } returns Result.success(Unit)

        bridge.updateFeed(ROOM, ChatFeedUpdate.New(coreMessage(ChatMessageBody.Text("hi"), request = true)), persist = true)

        val message = (update.captured as ProductFeedUpdateModel.New).message
        assertTrue(message.request)
        val welcome = requireNotNull((message.content as ChatMessage.Content.ChatRequest).welcome)
        assertEquals("hi", welcome.text)
    }

    @Test
    fun `contact events, calls and payments draw as the rows of the native person chat`() {
        assertEquals(ChatMessage.Content.ContactAdded, ChatMessageBody.Event(ChatContactEvent.CONTACT_ADDED).toContent())
        assertEquals(ChatMessage.Content.LeftChat, ChatMessageBody.Event(ChatContactEvent.LEFT_CHAT).toContent())
        assertTrue(ChatMessageBody.Event(ChatContactEvent.REQUEST_ACCEPTED).toContent() is ChatMessage.Content.ChatAccepted)
        assertEquals(
            ChatMessage.Content.ContactCall(video = true, answeredAt = 5, closedAt = 9, closedByUser = true),
            ChatMessageBody.Call(ChatCallRecord(video = true, answeredAt = 5uL, closed = ChatCallClose(at = 9uL, byUser = true))).toContent(),
        )
        assertEquals(
            ChatMessage.Content.ContactCall(video = false, answeredAt = null, closedAt = null, closedByUser = false),
            ChatMessageBody.Call(ChatCallRecord(video = false, answeredAt = null, closed = null)).toContent(),
        )
        val payment = ChatMessageBody.Payment(ChatPaymentRecord(amount = "1000", legacy = false)).toContent()
        assertEquals(1_000.toBigInteger(), (payment as ChatMessage.Content.CoinagePayment).totalValue.value)
        // The host draws its own status over a payment by message id; without one it reads as a native unprocessed payment.
        assertEquals(ChatMessage.Content.CoinagePayment.Status.Detecting, payment.status)
        // The native Android chat has no legacy Send message.
        assertTrue(ChatMessageBody.Payment(ChatPaymentRecord(amount = "1000", legacy = true)).toContent() is ChatMessage.Content.Unsupported)
    }

    @Test
    fun `a payment of the user goes to the product under the host's id, time, amount and keys`() {
        val keys = listOf(ByteArray(64) { 4 })
        val action = ChatUserActionModel.SendPayment("pay-1", timestamp = 1_234L, totalValue = 2_500.intoBalance(), coinKeys = keys).toCore()

        val payment = action as ChatUserAction.SendPayment
        assertEquals("pay-1", payment.messageId)
        assertEquals(1_234uL, payment.timestamp)
        assertEquals("2500", payment.totalValue)
        assertArrayEquals(keys.single(), payment.coinKeys.single())
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

        val preview = requireNotNull(reply.toModel().replyPreview)

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
