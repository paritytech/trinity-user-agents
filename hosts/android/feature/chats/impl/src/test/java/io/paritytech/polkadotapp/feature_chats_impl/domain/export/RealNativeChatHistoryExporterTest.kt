package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.mockk.coEvery
import io.mockk.mockk
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_impl.data.migration.NativeChatReader
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import kotlin.time.Instant

class RealNativeChatHistoryExporterTest {
    private val bob = DataByteArray(ByteArray(32) { 0xB0.toByte() })
    private fun key(byte: Int) = X25519PublicKey.fromBytes(DataByteArray(ByteArray(32) { byte.toByte() })).getOrThrow()

    private val contact = Contact(
        accountId = bob,
        username = "bob.01",
        chatKey = key(0xB1),
        ourMetaAccountId = 1,
        avatarUrl = null,
        sharedSecretDerivationDomain = SharedSecretDerivationDomain("chat"),
        pushToken = DataByteArray(byteArrayOf(1, 2, 3)),
        operatingSystem = OperatingSystem.IOS,
        isPeerLeft = true,
        pendingChatRequestId = "req-1",
        addedAt = Instant.fromEpochMilliseconds(1000),
    )
    private val reader = mockk<NativeChatReader> {
        coEvery { contacts() } returns listOf(contact)
        coEvery { devices(bob) } returns listOf(ContactDevice(bob, DataByteArray(ByteArray(32) { 0xD1.toByte() }), key(0xD2)))
        coEvery { request("req-1") } returns ChatRequest(
            welcomeMessageId = "req-1",
            timestamp = 2000,
            direction = ChatRequest.Direction.OUTGOING,
            status = ChatRequest.Status.PENDING,
            delivery = ChatRequest.Delivery.Delivered,
        )
    }
    private val sent = ChatMessage(
        id = "m-1",
        chatId = ChatId.fromContact(bob),
        timestamp = 3000,
        origin = ChatMessageOrigin.User,
        content = ChatMessage.Content.Text("hi"),
        status = ChatMessage.Status.IS_SENT,
    )

    // A message of a room that the host keeps for a product is no message of the native chat.
    private val botMessage = sent.copy(id = "b-1", chatId = ChatId.fromChatBotId("ProductBot_some.product"))

    private val messages = mockk<ChatMessageRepository> {
        coEvery { getMessagesUpdatedAfter(any(), any()) } returns listOf(sent, botMessage)
    }

    // The same state as the chat product's `HistoryExportCodec` encodes it: the product imports what Android writes.
    @Test
    fun `the export is the bytes the chat product imports`() = runTest {
        val export = RealNativeChatHistoryExporter(reader, messages).export()

        val wire = sent.toEncodedMessage().getOrThrow()
        assertEquals(
            "0004" + "b0".repeat(32) + "0118626f622e3031" + "b1".repeat(32) + "010c010203010100000" +
                "1e8030000000000000004" + "d1".repeat(32) + "d2".repeat(32) + "04147265712d31" + "b0".repeat(32) +
                "d0070000000000000100010004" + "b0".repeat(32) + compactLength(wire.size) + wire.toHexString() + "00" +
                "01b80b000000000000",
            export?.toHexString(),
        )
        // The message goes as the native wire carried it, so the product reads its id and time.
        val statement = BinaryScale.decodeFromByteArray<ChatMessageStatement>(wire)
        assertEquals("m-1", statement.id)
        assertEquals(3000uL, statement.timestamp)
    }

    @Test
    fun `a native chat without contacts has nothing to export`() = runTest {
        coEvery { reader.contacts() } returns emptyList()

        assertNull(RealNativeChatHistoryExporter(reader, messages).export())
    }

    private fun compactLength(length: Int): String = when {
        length < SINGLE_BYTE_LIMIT -> byteArrayOf((length shl 2).toByte()).toHexString()
        else -> byteArrayOf(((length shl 2) or 1).toByte(), ((length shl 2) shr 8).toByte()).toHexString()
    }

    private companion object {
        const val SINGLE_BYTE_LIMIT = 64
    }
}
