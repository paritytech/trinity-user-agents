package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.*
import org.junit.Assert.*
import org.junit.Ignore
import org.junit.Test

/**
 * Backward compatibility tests for ChatMessageContentLocal.
 *
 * These tests ensure that any changes to ChatMessageContentLocal remain backward compatible
 * by verifying that previously encoded data can still be decoded correctly.
 *
 * DO NOT modify the encoded hex values below - they represent the "frozen" format
 * that must always be decodable for backward compatibility.
 */
class ChatMessageContentLocalEncodingTest {
    @Test
    fun `decode Text content from encoded bytes`() {
        val encoded = "002c48656c6c6f20576f726c64"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.Text)
        assertEquals("Hello World", (decoded as ChatMessageContentLocal.Text).text)
    }

    @Test
    fun `decode Token Android content from encoded bytes`() {
        val encoded = "0128746573745f746f6b656e00"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.Token)
        val token = decoded as ChatMessageContentLocal.Token
        assertArrayEquals("test_token".toByteArray(), token.token)
        assertEquals(TokenPlatformLocal.ANDROID, token.platform)
    }

    @Test
    fun `decode Payment content from encoded bytes`() {
        // status is the sealed type: trailing 01 = Detected, followed by the detected Balance.
        val encoded = "10070010a5d4e80480000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f01070088526a74"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())
        assertTrue(decoded is ChatMessageContentLocal.CoinagePayment)
        val payment = decoded as ChatMessageContentLocal.CoinagePayment
        assertEquals(1000000000000.toBigInteger().intoBalance(), payment.totalValue)
        assertEquals(1, payment.coinKeys.size)
        val status = payment.status
        assertTrue(status is CoinagePaymentStatusLocal.Detected)
        assertEquals(500000000000.toBigInteger().intoBalance(), (status as CoinagePaymentStatusLocal.Detected).detected)
    }

    @Test
    fun `decode part-claimed Payment content from encoded bytes`() {
        // Identical to the Detected case above but for the sealed type's index: 05 = PartiallyClaimed. That
        // it only differs by that byte is the point — the variant was appended, so older rows still decode.
        val encoded = "10070010a5d4e80480000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f05070088526a74"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.CoinagePayment)
        val status = (decoded as ChatMessageContentLocal.CoinagePayment).status
        assertTrue(status is CoinagePaymentStatusLocal.PartiallyClaimed)
        assertEquals(
            500000000000.toBigInteger().intoBalance(),
            (status as CoinagePaymentStatusLocal.PartiallyClaimed).claimed
        )
    }

    @Test
    fun `decode ContactAdded content from encoded bytes`() {
        val encoded = "03"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.ContactAdded)
    }

    @Test
    fun `decode Reacted content from encoded bytes`() {
        val encoded = "041c6d73672d31323310f09f918d"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.Reacted)
        val reacted = decoded as ChatMessageContentLocal.Reacted
        assertEquals("msg-123", reacted.messageId)
        assertEquals(ChatMessageReactionContentLocal("👍"), reacted.content)
    }

    @Test
    fun `decode ReactionRemoved content from encoded bytes`() {
        val encoded = "051c6d73672d34353618e29da4efb88f"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.ReactionRemoved)
        val reactionRemoved = decoded as ChatMessageContentLocal.ReactionRemoved
        assertEquals("msg-456", reactionRemoved.messageId)
        assertEquals(ChatMessageReactionContentLocal("❤️"), reactionRemoved.content)
    }

    @Test
    fun `decode RichText with text only from encoded bytes`() {
        val encoded = "12011448656c6c6f00"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.RichText)
        val content = (decoded as ChatMessageContentLocal.RichText).content
        assertEquals("Hello", content.text)
        assertNull(content.attachments)
    }

    @Test
    fun `decode RichText with embedded image attachment from encoded bytes`() {
        val encoded = "120128436865636b20746869730104007468747470733a2f2f6578616d706c652e636f6d2f696d6167652e706e6700200300005802000024696d6167652f706e67000400000000000000"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.RichText)
        val content = (decoded as ChatMessageContentLocal.RichText).content
        assertEquals("Check this", content.text)
        assertEquals(1, content.attachments!!.size)

        val attachment = content.attachments[0]
        assertTrue(attachment is AttachmentLocal.Embedded)
        val embedded = attachment as AttachmentLocal.Embedded
        assertEquals("https://example.com/image.png", embedded.uri)

        val meta = embedded.meta
        assertTrue(meta is AttachmentMetaLocal.Image)
        val image = meta as AttachmentMetaLocal.Image
        assertEquals(800, image.width)
        assertEquals(600, image.height)
        assertEquals("image/png", image.mimeType)
        assertEquals(1024, image.sizeBytes)
    }

    @Test
    fun `decode RichText with hosted file attachment from encoded bytes`() {
        val encoded = "1201205365652066696c65010401000230646f63756d656e742e7064663c6170706c69636174696f6e2f706466002003000000000080000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f80202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f587773733a2f2f686f702e6578616d706c652f6e6f6465"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.RichText)
        val content = (decoded as ChatMessageContentLocal.RichText).content
        assertEquals("See file", content.text)
        assertEquals(1, content.attachments!!.size)

        val attachment = content.attachments!![0]
        assertTrue(attachment is AttachmentLocal.Hosted)
        val hosted = attachment as AttachmentLocal.Hosted
        assertNull(hosted.uri)
        assertArrayEquals(ByteArray(32) { it.toByte() }, hosted.identifier)
        assertArrayEquals(ByteArray(32) { (it + 0x20).toByte() }, hosted.claimTicket)
        assertEquals("wss://hop.example/node", hosted.nodeUrl)

        val meta = hosted.meta
        assertTrue(meta is AttachmentMetaLocal.General)
        val general = meta as AttachmentMetaLocal.General
        assertEquals("document.pdf", general.fileName)
        assertEquals("application/pdf", general.mimeType)
        assertEquals(204800, general.sizeBytes)
    }

    @Test
    fun `decode Unsupported content from encoded bytes`() {
        val encoded = "0710fffe0102"
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(encoded.fromHex())

        assertTrue(decoded is ChatMessageContentLocal.Unsupported)
        val unsupported = decoded as ChatMessageContentLocal.Unsupported
        assertArrayEquals(byteArrayOf(0xFF.toByte(), 0xFE.toByte(), 0x01, 0x02), unsupported.rawContent)
    }

    /**
     * Helper test to print encoded values for all content types.
     * Run this test to generate new encoded values when adding new content types.
     * The output can be used to create new backward compatibility tests.
     */
    @Ignore // Run manually
    @Test
    fun printEncodedValues() {
        val values = mapOf(
            "Text" to ChatMessageContentLocal.Text("Hello World"),
            "Token" to ChatMessageContentLocal.Token(
                token = "test_token".toByteArray(),
                platform = TokenPlatformLocal.ANDROID
            ),
            "Payment" to ChatMessageContentLocal.CoinagePayment(
                totalValue = 1000000000000.toBigInteger().intoBalance(),
                coinKeys = listOf(ByteArray(32) { it.toByte() }),
                status = CoinagePaymentStatusLocal.Detected(500000000000.toBigInteger().intoBalance())
            ),
            "Payment (partially claimed)" to ChatMessageContentLocal.CoinagePayment(
                totalValue = 1000000000000.toBigInteger().intoBalance(),
                coinKeys = listOf(ByteArray(32) { it.toByte() }),
                status = CoinagePaymentStatusLocal.PartiallyClaimed(500000000000.toBigInteger().intoBalance())
            ),
            "Payment (partially claimed)" to ChatMessageContentLocal.CoinagePayment(
                totalValue = 1000000000000.toBigInteger().intoBalance(),
                coinKeys = listOf(ByteArray(32) { it.toByte() }),
                status = CoinagePaymentStatusLocal.PartiallyClaimed(500000000000.toBigInteger().intoBalance())
            ),
            "ContactAdded" to ChatMessageContentLocal.ContactAdded,
            "Reacted" to ChatMessageContentLocal.Reacted(
                messageId = "msg-123",
                content = ChatMessageReactionContentLocal("👍")
            ),
            "ReactionRemoved" to ChatMessageContentLocal.ReactionRemoved(
                messageId = "msg-456",
                content = ChatMessageReactionContentLocal("❤️")
            ),
            "Unsupported" to ChatMessageContentLocal.Unsupported(
                rawContent = byteArrayOf(0xFF.toByte(), 0xFE.toByte(), 0x01, 0x02)
            ),
            "RichText (text only)" to ChatMessageContentLocal.RichText(
                content = RichTextContentLocal(text = "Hello", attachments = null)
            ),
            "RichText (with hosted file)" to ChatMessageContentLocal.RichText(
                content = RichTextContentLocal(
                    text = "See file",
                    attachments = listOf(
                        AttachmentLocal.Hosted(
                            uri = null,
                            meta = AttachmentMetaLocal.General(
                                fileName = "document.pdf",
                                mimeType = "application/pdf",
                                sizeBytes = 204800
                            ),
                            identifier = ByteArray(32) { it.toByte() },
                            claimTicket = ByteArray(32) { (it + 0x20).toByte() },
                            nodeUrl = "wss://hop.example/node"
                        )
                    )
                )
            ),
            "RichText (with embedded image)" to ChatMessageContentLocal.RichText(
                content = RichTextContentLocal(
                    text = "Check this",
                    attachments = listOf(
                        AttachmentLocal.Embedded(
                            uri = "https://example.com/image.png",
                            meta = AttachmentMetaLocal.Image(
                                width = 800,
                                height = 600,
                                mimeType = "image/png",
                                sizeBytes = 1024,
                                blurHash = null
                            )
                        )
                    )
                )
            )
        )

        println("===========================================")
        values.forEach { (key, value) ->
            println("$key: ${value.scaleEncodeBinary().toHexString()}")
        }
        println("===========================================")
    }
}
