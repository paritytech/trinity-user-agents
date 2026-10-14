package io.paritytech.polkadotapp.feature_chats_impl.data.notifications

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.DataChannelPurpose
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NotificationMessagePayload
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.StrippedChatMessageContentV1
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.VersionedNotificationChatMessageContent
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatNotificationPayloadEncoderTest {
    private val encoder = ChatNotificationPayloadEncoder()

    private val contactAccountId = ByteArray(32) { 1 }.intoAccountId()

    @Test
    fun `small text is sent in full`() {
        val content = encodeAndDecode(message(ChatMessage.Content.Text("hello")))

        val full = content as NotificationChatMessageContentV1.Full
        assertEquals("hello", (full.content as ChatMessageStatementContent.Text).text)
    }

    @Test
    fun `payment with a few coins is sent in full with its coin keys`() {
        val content = encodeAndDecode(message(payment(coinCount = 3)))

        val full = content as NotificationChatMessageContentV1.Full
        assertEquals(3, (full.content as ChatMessageStatementContent.CoinagePayment).coinKeys.size)
    }

    @Test
    fun `payment with many coins is stripped down to its total`() {
        val content = encodeAndDecode(message(payment(coinCount = 100)))

        val stripped = (content as NotificationChatMessageContentV1.Stripped).content
        assertEquals(TOTAL_VALUE.intoBalance(), (stripped as StrippedChatMessageContentV1.CoinagePayment).totalBalance)
    }

    @Test
    fun `oversized call offer is stripped and keeps its purpose`() {
        val sdp = ByteArray(2500) { 7 }

        val content = encodeAndDecode(
            message(ChatMessage.Content.DataChannelOffer(sdp, ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL))
        )

        val offer = (content as NotificationChatMessageContentV1.Stripped).content as StrippedChatMessageContentV1.DataChannelOffer
        assertEquals(DataChannelPurpose.VIDEO_CALL, offer.purpose)
        assertArrayEquals(sdp, offer.sdp)
    }

    @Test
    fun `oversized text is still sent stripped`() {
        val text = "a".repeat(3000)

        val content = encodeAndDecode(message(ChatMessage.Content.Text(text)))

        val stripped = (content as NotificationChatMessageContentV1.Stripped).content
        assertEquals(text, (stripped as StrippedChatMessageContentV1.Text).text)
    }

    private fun encodeAndDecode(message: ChatMessage): NotificationChatMessageContentV1 {
        val encoded = encoder.encode(message).getOrThrow()
        val payload = BinaryScale.decodeFromByteArray<NotificationMessagePayload>(encoded)

        assertEquals(message.id, payload.id)
        assertEquals(message.timestamp.toULong(), payload.timestamp)

        return (payload.message.versioned as VersionedNotificationChatMessageContent.V1).content
    }

    private fun payment(coinCount: Int) = ChatMessage.Content.CoinagePayment(
        totalValue = TOTAL_VALUE.intoBalance(),
        coinKeys = List(coinCount) { index -> ByteArray(32) { index.toByte() } },
        status = ChatMessage.Content.CoinagePayment.Status.Detecting
    )

    private fun message(content: ChatMessage.Content) = ChatMessage(
        id = "message-id",
        chatId = ChatId.fromContact(contactAccountId),
        timestamp = 1_700_000_000_000L,
        origin = ChatMessageOrigin.User,
        content = content,
        status = ChatMessage.Status.IS_SENT,
    )

    private companion object {
        const val TOTAL_VALUE = 5_000_000L
    }
}
