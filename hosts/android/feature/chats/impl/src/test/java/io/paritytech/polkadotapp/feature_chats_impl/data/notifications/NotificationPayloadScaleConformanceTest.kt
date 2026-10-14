package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationMessagePayload
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.StrippedChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedNotificationChatMessageContent
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Cross-platform conformance vectors for the push notification payload. The hex values are frozen:
 * they pin the byte layout iOS produces/expects, so any codec drift fails here instead of as a lost push.
 */
class NotificationPayloadScaleConformanceTest {
    private val coinKey = ByteArray(32) { it.toByte() }
    private val totalBalance = 1_000_000_000_000L.intoBalance()

    // id "m1", timestamp 1, V1
    private val header = "08" + "6d31" + "0100000000000000" + "00"

    private val fullTextHex = header + "01" + "00" + "08" + "6869"

    private val strippedTextHex = header + "00" + "00" + "08" + "6869"

    private val fullPaymentHex = header + "01" + "10" + "070010a5d4e8" +
        "04" + "80" + "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"

    private val strippedPaymentHex = header + "00" + "10" + "070010a5d4e8"

    @Test
    fun `full text encodes to the frozen vector`() {
        val encoded = encode(NotificationChatMessageContentV1.Full(ChatMessageStatementContent.Text("hi")))

        assertEquals(fullTextHex, encoded)
    }

    @Test
    fun `stripped text encodes to the frozen vector`() {
        val encoded = encode(NotificationChatMessageContentV1.Stripped(StrippedChatMessageContentV1.Text("hi")))

        assertEquals(strippedTextHex, encoded)
    }

    @Test
    fun `full payment encodes to the frozen vector`() {
        val encoded = encode(
            NotificationChatMessageContentV1.Full(ChatMessageStatementContent.CoinagePayment(totalBalance, listOf(coinKey)))
        )

        assertEquals(fullPaymentHex, encoded)
    }

    @Test
    fun `stripped payment encodes to the frozen vector`() {
        val encoded = encode(NotificationChatMessageContentV1.Stripped(StrippedChatMessageContentV1.CoinagePayment(totalBalance)))

        assertEquals(strippedPaymentHex, encoded)
    }

    @Test
    fun `full payment decodes from the frozen vector`() {
        val payload = decode(fullPaymentHex)

        assertEquals("m1", payload.id)
        assertEquals(1uL, payload.timestamp)
        val payment = (payload.content() as NotificationChatMessageContentV1.Full).content as ChatMessageStatementContent.CoinagePayment
        assertEquals(totalBalance, payment.totalValue)
        assertArrayEquals(coinKey, payment.coinKeys.single())
    }

    @Test
    fun `stripped payment decodes from the frozen vector`() {
        val payload = decode(strippedPaymentHex)

        assertEquals("m1", payload.id)
        assertEquals(1uL, payload.timestamp)
        val payment = (payload.content() as NotificationChatMessageContentV1.Stripped).content as StrippedChatMessageContentV1.CoinagePayment
        assertEquals(totalBalance, payment.totalBalance)
    }

    @Test
    fun `stripped text decodes from the frozen vector`() {
        val payload = decode(strippedTextHex)

        val text = (payload.content() as NotificationChatMessageContentV1.Stripped).content as StrippedChatMessageContentV1.Text
        assertEquals("hi", text.text)
    }

    private fun encode(content: NotificationChatMessageContentV1): String {
        val payload = NotificationMessagePayload(
            id = "m1",
            timestamp = 1uL,
            message = NotificationChatMessage(VersionedNotificationChatMessageContent.V1(content))
        )

        return BinaryScale.encodeToByteArray(payload).toHexString()
    }

    private fun decode(hex: String): NotificationMessagePayload {
        return BinaryScale.decodeFromByteArray<NotificationMessagePayload>(hex.fromHex())
    }

    private fun NotificationMessagePayload.content(): NotificationChatMessageContentV1 {
        return (message.versioned as VersionedNotificationChatMessageContent.V1).content
    }
}
