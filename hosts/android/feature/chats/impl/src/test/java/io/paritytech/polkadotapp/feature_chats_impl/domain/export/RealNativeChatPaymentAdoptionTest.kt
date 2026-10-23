package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage.Content.CoinagePayment
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_impl.data.migration.NativeChatReader
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.extension.CoinagePaymentProcessingExtension
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.FakeProductChatPaymentStore
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPayment
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigInteger

/**
 * The native chat's payment messages become the host's payment records of the chat product when the install moves to
 * it: the same id, time, amount, keys and status, in the contact's room of the product, open exactly when the native
 * coinage flow had not finished them.
 */
class RealNativeChatPaymentAdoptionTest {
    private val bob = DataByteArray(ByteArray(32) { 0xB0.toByte() })
    private val bobRoom = ChatId.forExtensionRoom("ProductBot_chat.paseo", "dm-" + "b0".repeat(32))
    private val roomOf: (AccountId) -> ChatId = { account -> ChatId.forExtensionRoom("ProductBot_chat.paseo", "dm-" + account.value.joinToString("") { "%02x".format(it) }) }

    private val store = FakeProductChatPaymentStore()
    private val unfinished = mutableSetOf<String>()
    private val reader = mockk<NativeChatReader> {
        coEvery { unprocessedMessageIds(CoinagePaymentProcessingExtension.ID) } answers { unfinished.toSet() }
    }
    private val stored = mutableListOf<ChatMessage>()
    private val messages = mockk<ChatMessageRepository> {
        coEvery { getMessagesUpdatedAfter(any(), any()) } answers { stored.toList() }
    }
    private val adoption = RealNativeChatPaymentAdoption(reader, messages, store)

    @Test
    fun `an incoming payment the native flow did not finish is kept open, as it was, in the contact's room`() = runTest {
        stored += payment("in-1", incoming = true, status = CoinagePayment.Status.Detected(FULL))
        unfinished += "in-1"

        assertEquals(1, adoption.adopt(roomOf))

        val record = store["in-1"]!!
        assertEquals(bobRoom, record.chatId)
        assertTrue(record.incoming)
        assertEquals(SENT_AT, record.timestamp)
        assertEquals(FULL, record.totalValue)
        assertArrayEquals(byteArrayOf(1), record.coinKeys.single())
        assertNull(record.delivery)
        assertEquals(CoinagePayment.Status.Detected(FULL), record.status)
        assertFalse(record.settled)
    }

    @Test
    fun `a payment the native flow finished is kept settled with its last status`() = runTest {
        stored += payment("out-1", incoming = false, status = CoinagePayment.Status.Transferred(FULL))

        adoption.adopt(roomOf)

        val record = store["out-1"]!!
        assertFalse(record.incoming)
        // The chat product has the message from the history import, so the host never sends it again.
        assertEquals(ProductChatPayment.Delivery.DELIVERED, record.delivery)
        assertEquals(CoinagePayment.Status.Transferred(FULL), record.status)
        assertTrue(record.settled)
    }

    @Test
    fun `an outgoing payment whose coins the native flow still followed is followed on`() = runTest {
        stored += payment("out-2", incoming = false, status = CoinagePayment.Status.Detecting)
        unfinished += "out-2"

        adoption.adopt(roomOf)

        assertFalse(store["out-2"]!!.settled)
    }

    @Test
    fun `a payment without keys has nothing to claim and is kept settled`() = runTest {
        stored += payment("in-2", incoming = true, status = CoinagePayment.Status.Detecting, keys = emptyList())
        unfinished += "in-2"

        adoption.adopt(roomOf)

        assertTrue(store["in-2"]!!.settled)
    }

    @Test
    fun `only payments of contact chats are taken`() = runTest {
        stored += payment("in-1", incoming = true, status = CoinagePayment.Status.Detecting)
        stored += payment("bot-1", incoming = true, status = CoinagePayment.Status.Detecting).copy(chatId = ChatId.fromChatBotId("ProductBot_some.product"))
        stored += payment("text-1", incoming = true, status = CoinagePayment.Status.Detecting).copy(content = ChatMessage.Content.Text("hi"))

        assertEquals(1, adoption.adopt(roomOf))
        assertEquals(setOf("in-1"), store.records.value.keys)
    }

    /** A second run, or a run after the host's flow moved a record on, keeps nothing and overwrites nothing. */
    @Test
    fun `taking the payments over again changes nothing`() = runTest {
        stored += payment("in-1", incoming = true, status = CoinagePayment.Status.Detecting)
        unfinished += "in-1"
        adoption.adopt(roomOf)
        store.setStatus("in-1", CoinagePayment.Status.Transferred(FULL))
        store.settle("in-1")

        assertEquals(0, adoption.adopt(roomOf))
        assertEquals(CoinagePayment.Status.Transferred(FULL), store["in-1"]!!.status)
        assertTrue(store["in-1"]!!.settled)
    }

    private fun payment(id: String, incoming: Boolean, status: CoinagePayment.Status, keys: List<ByteArray> = listOf(byteArrayOf(1))) = ChatMessage(
        id = id,
        chatId = ChatId.fromContact(bob),
        timestamp = SENT_AT,
        origin = if (incoming) ChatMessageOrigin.Contact(bob) else ChatMessageOrigin.User,
        content = CoinagePayment(totalValue = FULL, coinKeys = keys, status = status),
        status = ChatMessage.Status.IS_READ,
    )

    private companion object {
        const val SENT_AT = 1_700_000_000_000L
        val FULL = BigInteger.valueOf(100).intoBalance()
    }
}
