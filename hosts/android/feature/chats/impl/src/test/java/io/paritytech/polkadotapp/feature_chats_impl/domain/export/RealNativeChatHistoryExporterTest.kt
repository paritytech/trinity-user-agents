package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.mockk.coEvery
import io.mockk.mockk
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.ApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
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
    private val contacts = mockk<ContactsRepository> { coEvery { getContacts() } returns listOf(contact) }
    private val devices = mockk<ContactDevicesRepository> {
        coEvery { getDevices(bob) } returns listOf(ContactDevice(bob, DataByteArray(ByteArray(32) { 0xD1.toByte() }), key(0xD2)))
    }
    private val requests = mockk<ChatRequestRepository> {
        coEvery { getById("req-1") } returns ChatRequest(
            welcomeMessageId = "req-1",
            timestamp = 2000,
            direction = ChatRequest.Direction.OUTGOING,
            status = ChatRequest.Status.PENDING,
            delivery = ChatRequest.Delivery.Delivered,
        )
    }
    private val messages = mockk<ApplyRemoteChatMessageUseCase> {
        coEvery { getMessagesUpdatedAfter(any()) } returns listOf(
            ApplyRemoteChatMessageUseCase.MessageForSync(
                encoded = byteArrayOf(9, 9),
                peerAccountId = bob,
                isOutgoing = true,
                status = ChatMessage.Status.IS_SENT,
                timestamp = 3000,
            )
        )
    }

    // The same state as the chat product's `HistoryExportCodec` encodes it: the product imports what Android writes.
    @Test
    fun `the export is the bytes the chat product imports`() = runTest {
        val export = RealNativeChatHistoryExporter(contacts, devices, requests, messages).export()

        assertEquals(
            "0004" + "b0".repeat(32) + "0118626f622e3031" + "b1".repeat(32) + "010c010203010100000" +
                "1e8030000000000000004" + "d1".repeat(32) + "d2".repeat(32) + "04147265712d31" + "b0".repeat(32) +
                "d0070000000000000100010004" + "b0".repeat(32) + "08090900" + "01b80b000000000000",
            export?.toHexString(),
        )
    }

    @Test
    fun `a native chat without contacts has nothing to export`() = runTest {
        coEvery { contacts.getContacts() } returns emptyList()

        assertNull(RealNativeChatHistoryExporter(contacts, devices, requests, messages).export())
    }
}
