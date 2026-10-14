package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.ChatPushContent
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.MESSAGE_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder.Companion.PUSH_ID_KEY
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.StrippedChatPushContent
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationMessagePayload
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.StrippedChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedNotificationChatMessageContent
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class RealIncomingChatPushDecoderTest {
    private val contactsRepository: ContactsRepository = mockk()
    private val encryptionFactory: CommunicationEncryption.Factory = mockk()

    private val decoder = RealIncomingChatPushDecoder(contactsRepository, encryptionFactory)

    private val contactAccountId = ByteArray(32) { 1 }.intoAccountId()

    @Before
    fun setUp() {
        withKnownContact()
        withPassthroughEncryption()
    }

    @Test
    fun `full payload is decoded into a full message`() = runBlocking<Unit> {
        val decoded = decode(payload(NotificationChatMessageContentV1.Full(ChatMessageStatementContent.Text("hi"))))

        val full = decoded.content as ChatPushContent.Full
        assertEquals(MESSAGE_ID, full.message.id)
        assertEquals("hi", (full.message.content as ChatMessage.Content.Text).text)
    }

    @Test
    fun `stripped payment is decoded into its total only`() = runBlocking<Unit> {
        val decoded = decode(
            payload(NotificationChatMessageContentV1.Stripped(StrippedChatMessageContentV1.CoinagePayment(TOTAL_VALUE.intoBalance())))
        )

        val stripped = decoded.content as ChatPushContent.Stripped
        assertEquals(MESSAGE_ID, stripped.messageId)
        assertEquals(TOTAL_VALUE.intoBalance(), (stripped.content as StrippedChatPushContent.CoinagePayment).totalValue)
    }

    @Test
    fun `stripped text is decoded into regular content`() = runBlocking<Unit> {
        val decoded = decode(payload(NotificationChatMessageContentV1.Stripped(StrippedChatMessageContentV1.Text("hi"))))

        val regular = (decoded.content as ChatPushContent.Stripped).content as StrippedChatPushContent.Regular
        assertEquals("hi", (regular.content as ChatMessage.Content.Text).text)
    }

    @Test
    fun `undecodable content falls back to stripped unsupported`() = runBlocking<Unit> {
        val validPrefix = BinaryScale.encodeToByteArray(MESSAGE_ID) + BinaryScale.encodeToByteArray(TIMESTAMP)
        val unknownVersion = byteArrayOf(0x7F, 0x01, 0x02)

        val decoded = decode(validPrefix + unknownVersion)

        val stripped = decoded.content as ChatPushContent.Stripped
        assertEquals(MESSAGE_ID, stripped.messageId)
        assertTrue((stripped.content as StrippedChatPushContent.Regular).content is ChatMessage.Content.Unsupported)
    }

    private suspend fun decode(encoded: ByteArray) = decoder.decode(
        mapOf(
            PUSH_ID_KEY to ByteArray(32).toHexString(),
            MESSAGE_KEY to encoded.toHexString(),
        )
    ).getOrThrow()

    private fun payload(content: NotificationChatMessageContentV1): ByteArray {
        val payload = NotificationMessagePayload(
            id = MESSAGE_ID,
            timestamp = TIMESTAMP,
            message = NotificationChatMessage(VersionedNotificationChatMessageContent.V1(content)),
        )

        return BinaryScale.encodeToByteArray(payload)
    }

    private fun withKnownContact() {
        val contact = mockk<Contact> {
            every { accountId } returns contactAccountId
            every { isBlocked } returns false
            every { sharedSecretDerivationDomain } returns SharedSecretDerivationDomain("test")
            every { chatKey } returns X25519PublicKey.fromBytes(ByteArray(32).toDataByteArray()).getOrThrow()
        }
        coEvery { contactsRepository.getContactByPushId(any()) } returns contact
    }

    private fun withPassthroughEncryption() {
        val encryption = mockk<CommunicationEncryption> {
            every { decrypt(any()) } answers { firstArg() }
        }
        coEvery { encryptionFactory.create(any(), any()) } returns encryption
    }

    private companion object {
        const val MESSAGE_ID = "message-id"
        const val TIMESTAMP = 1_700_000_000_000uL
        const val TOTAL_VALUE = 5_000_000L
    }
}
