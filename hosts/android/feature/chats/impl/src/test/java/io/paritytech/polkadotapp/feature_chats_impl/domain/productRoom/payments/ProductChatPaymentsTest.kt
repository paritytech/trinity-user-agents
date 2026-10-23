package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment.Status
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.NoProductChatRoomException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatFeedSnapshot
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatPagingState
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ChatRoomReadiness
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStageScheduler
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductChatPaymentsTest {
    private val contact: AccountId = ByteArray(32) { 1 }.toDataByteArray()
    private val room = ChatId.forExtensionRoom("ProductBot_chat.paseo", "dm-01")
    private val otherProductRoom = ChatId.forExtensionRoom("ProductBot_other.paseo", "dm-01")

    private val store = FakeProductChatPaymentStore()
    private val scheduler = object : ProductAttachmentStageScheduler {
        val scheduled = mutableListOf<ChatId>()

        override suspend fun schedule(chatId: ChatId) {
            scheduled += chatId
        }
    }
    private var rooms = mapOf(room to declared(ChatContactStateModel.ESTABLISHED))
    private val declarations = mockk<ChatRoomDeclarationsRepository> { every { observeContactRooms() } answers { flowOf(rooms) } }
    private val personChatRooms = object : PersonChatRooms {
        override fun homeRoom(): ChatId? = null

        override fun isPersonChatRoom(chatId: ChatId): Boolean = chatId == room
    }
    private val payments = RealProductChatPayments(store, declarations, personChatRooms, scheduler)

    // ---- sending ----

    @Test
    fun `a payment commits its coins inside the transaction that keeps it, then goes to the contact's room`() = runBlocking {
        val keys = listOf(ByteArray(64) { 5 }, ByteArray(64) { 6 })
        var keptWhenCommitted: Boolean? = null

        payments.send(contact, 2_500.intoBalance(), keys, onSaved = { keptWhenCommitted = store.records.value.isNotEmpty() })

        val kept = store.records.value.values.single()
        // The fake keeps the record after onSaved, as the transaction makes both durable at once.
        assertEquals(false, keptWhenCommitted)
        assertEquals(room, kept.chatId)
        assertFalse(kept.incoming)
        assertEquals(2_500.intoBalance(), kept.totalValue)
        assertEquals(2, kept.coinKeys.size)
        assertArrayEquals(keys[1], kept.coinKeys[1])
        assertEquals(ProductChatPayment.Delivery.SENDING, kept.delivery)
        assertEquals(Status.Detecting, kept.status)
        assertEquals(listOf(room), scheduler.scheduled)
    }

    @Test
    fun `a refused commit keeps nothing and sends nothing`() = runBlocking {
        val refused = runCatching {
            payments.send(contact, 1.intoBalance(), listOf(ByteArray(64)), onSaved = { error("handed off meanwhile") })
        }

        assertTrue(refused.isFailure)
        assertTrue(store.records.value.isEmpty())
        assertTrue(scheduler.scheduled.isEmpty())
    }

    @Test
    fun `without an established chat with the recipient nothing is committed`() = runBlocking {
        rooms = mapOf(room to declared(ChatContactStateModel.OUTGOING), otherProductRoom to declared(ChatContactStateModel.ESTABLISHED))
        var committed = false

        val result = runCatching { payments.send(contact, 1.intoBalance(), listOf(ByteArray(64)), onSaved = { committed = true }) }

        assertTrue(result.exceptionOrNull() is NoProductChatRoomException)
        assertFalse(committed)
        assertTrue(store.records.value.isEmpty())
    }

    @Test
    fun `payments that wait for the product go again at start`() = runBlocking {
        store.put(payment("pay-1", incoming = false, delivery = ProductChatPayment.Delivery.SENDING))
        store.put(payment("pay-2", incoming = false, delivery = ProductChatPayment.Delivery.DELIVERED))

        payments.resumeDeliveries()

        assertEquals(listOf(room), scheduler.scheduled)
    }

    // ---- receiving ----

    @Test
    fun `a received payment is kept once by its message id`() = runBlocking {
        val keys = listOf(ByteArray(64) { 9 })

        payments.receive(room, "in-1", 7_000L, 300.intoBalance(), keys)
        store.setStatus("in-1", Status.Detected(300.intoBalance()))
        payments.receive(room, "in-1", 7_000L, 300.intoBalance(), keys)

        val kept = store["in-1"]!!
        assertTrue(kept.incoming)
        assertNull(kept.delivery)
        assertEquals(7_000L, kept.timestamp)
        assertEquals(Status.Detected(300.intoBalance()), kept.status)
        assertEquals(1, store.records.value.size)
    }

    @Test
    fun `a received payment without keys is not kept`() = runBlocking {
        payments.receive(room, "in-1", 7_000L, 300.intoBalance(), emptyList())

        assertTrue(store.records.value.isEmpty())
    }

    // ---- the room ----

    @Test
    fun `a payment in the window shows the status that the host keeps for it`() {
        val window = snapshot(windowPayment("in-1", 500, ChatMessageOrigin.Extension("ProductBot_chat.paseo")))
        val claimed = payment("in-1", incoming = true, delivery = null).copy(status = Status.Transferred(1_500.intoBalance()))

        val shown = window.withPayments(listOf(claimed))

        val content = shown.messages.single().content as ChatMessage.Content.CoinagePayment
        assertEquals(Status.Transferred(1_500.intoBalance()), content.status)
    }

    @Test
    fun `a payment of the user that waits for the product shows at its time in the native pending state`() {
        val window = snapshot(windowText("w2", 350), windowText("w1", 200))
        val sending = payment("pay-1", incoming = false, delivery = ProductChatPayment.Delivery.SENDING, timestamp = 300)
        val refused = payment("pay-2", incoming = false, delivery = ProductChatPayment.Delivery.REFUSED, timestamp = 400)

        val shown = window.withPayments(listOf(sending, refused))

        assertEquals(listOf("pay-2", "w2", "pay-1", "w1"), shown.messages.map { it.id })
        val byId = shown.messages.associateBy { it.id }
        assertEquals(ChatMessage.Status.NEW, byId.getValue("pay-1").status)
        assertEquals(ChatMessage.Status.DELIVERY_FAILED, byId.getValue("pay-2").status)
        assertSame(ChatMessageOrigin.User, byId.getValue("pay-1").origin)
        assertEquals(ProductMessageStatusModel.PREPARING, shown.productStatuses["pay-1"])
        assertEquals(ProductMessageStatusModel.FAILED, shown.productStatuses["pay-2"])
    }

    @Test
    fun `once the window has the message of a payment, it shows that message`() {
        val window = snapshot(windowPayment("pay-1", 300, ChatMessageOrigin.User))
        val sending = payment("pay-1", incoming = false, delivery = ProductChatPayment.Delivery.SENDING, timestamp = 300)

        val shown = window.withPayments(listOf(sending))

        assertEquals(listOf("pay-1"), shown.messages.map { it.id })
        assertTrue(shown.productStatuses.isEmpty())
    }

    private fun declared(state: ChatContactStateModel) =
        ChatRoomDeclarations.EMPTY.copy(contact = ChatRoomContactModel(contact, username = "alice", state = state))

    private fun payment(
        id: String,
        incoming: Boolean,
        delivery: ProductChatPayment.Delivery?,
        timestamp: Long = 100,
    ) = ProductChatPayment(
        messageId = id,
        chatId = room,
        incoming = incoming,
        timestamp = timestamp,
        totalValue = 1_500.intoBalance(),
        coinKeys = listOf(ByteArray(64) { 2 }),
        delivery = delivery,
        refusals = 0,
        status = Status.Detecting,
        settled = false,
    )

    private fun snapshot(vararg messages: ChatMessage) = ChatFeedSnapshot(
        messages = messages.toList(),
        reactions = emptyMap(),
        revisions = emptyMap(),
        productStatuses = emptyMap(),
        paging = ChatPagingState.Complete,
        readiness = ChatRoomReadiness.READY,
    )

    private fun windowText(id: String, timestamp: Long) = ChatMessage(
        id = id,
        chatId = room,
        timestamp = timestamp,
        origin = ChatMessageOrigin.Extension("ProductBot_chat.paseo"),
        content = ChatMessage.Content.Text(id),
        status = ChatMessage.Status.IS_READ,
    )

    private fun windowPayment(id: String, timestamp: Long, origin: ChatMessageOrigin) = ChatMessage(
        id = id,
        chatId = room,
        timestamp = timestamp,
        origin = origin,
        content = ChatMessage.Content.CoinagePayment(totalValue = 1_500.intoBalance(), coinKeys = emptyList(), status = Status.Detecting),
        status = ChatMessage.Status.IS_READ,
    )
}
