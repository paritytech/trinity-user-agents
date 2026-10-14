package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatActionButtonLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatActionsLayoutLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMediaItemLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMediaMetaLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatResourceLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ProductMessageBodyLocal
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Ignore
import org.junit.Test

/**
 * Backward compatibility of the Chat Extension v2 variants of [ChatMessageContentLocal].
 *
 * DO NOT modify the encoded hex values below: they are the frozen format that must always decode.
 */
class ProductContentLocalEncodingTest {
    @Test
    fun `decode ProductBody rich text with inline, preimage and url media from encoded bytes`() {
        val decoded = decode(RICH_TEXT)

        val body = (decoded as ChatMessageContentLocal.ProductBody).body as ProductMessageBodyLocal.RichText
        assertEquals("Look", body.text)
        assertEquals(3, body.media.size)

        val image = body.media[0] as ChatMediaItemLocal.Image
        val inline = image.resource as ChatResourceLocal.Inline
        assertEquals("image/png", inline.mimeType)
        assertArrayEquals(byteArrayOf(1, 2, 3), inline.bytes)
        assertEquals(3L, image.meta.sizeBytes)
        assertEquals(2, image.meta.width)
        assertEquals(1, image.meta.height)
        assertEquals("LKO2", image.meta.placeholder)
        assertEquals("dot", image.meta.alt)

        val file = body.media[1] as ChatMediaItemLocal.File
        assertArrayEquals(ByteArray(32) { it.toByte() }, (file.resource as ChatResourceLocal.Preimage).key)
        assertEquals("application/pdf", file.meta.mimeType)
        assertNull(file.meta.sizeBytes)
        assertEquals("a.pdf", file.fileName)

        val video = body.media[2] as ChatMediaItemLocal.Video
        assertEquals("https://example.com/v.mp4", (video.resource as ChatResourceLocal.Url).url)
    }

    @Test
    fun `decode ProductBody actions from encoded bytes`() {
        val decoded = decode(ACTIONS)

        val body = (decoded as ChatMessageContentLocal.ProductBody).body as ProductMessageBodyLocal.Actions
        assertEquals("Pick", body.text)
        assertEquals(listOf("yes" to "Yes", "no" to "No"), body.actions.map { it.actionId to it.title })
        assertEquals(ChatActionsLayoutLocal.GRID, body.layout)
    }

    @Test
    fun `decode Deleted tombstone from encoded bytes`() {
        assertTrue(decode(DELETED) is ChatMessageContentLocal.Deleted)
    }

    @Ignore("Run manually to print the hex of a new value")
    @Test
    fun printEncodedValues() {
        val values = mapOf(
            "RICH_TEXT" to ChatMessageContentLocal.ProductBody(
                ProductMessageBodyLocal.RichText(
                    text = "Look",
                    media = listOf(
                        ChatMediaItemLocal.Image(
                            ChatResourceLocal.Inline("image/png", byteArrayOf(1, 2, 3)),
                            ChatMediaMetaLocal("image/png", 3, 2, 1, "LKO2", "dot"),
                        ),
                        ChatMediaItemLocal.File(
                            ChatResourceLocal.Preimage(ByteArray(32) { it.toByte() }),
                            ChatMediaMetaLocal("application/pdf", null, null, null, null, null),
                            "a.pdf",
                        ),
                        ChatMediaItemLocal.Video(
                            ChatResourceLocal.Url("https://example.com/v.mp4"),
                            ChatMediaMetaLocal("video/mp4", null, null, null, null, null),
                        ),
                    ),
                ),
            ),
            "ACTIONS" to ChatMessageContentLocal.ProductBody(
                ProductMessageBodyLocal.Actions(
                    text = "Pick",
                    actions = listOf(ChatActionButtonLocal("yes", "Yes"), ChatActionButtonLocal("no", "No")),
                    layout = ChatActionsLayoutLocal.GRID,
                ),
            ),
            "DELETED" to ChatMessageContentLocal.Deleted,
        )
        values.forEach { (name, value) ->
            println("$name = ${BinaryScale.encodeToByteArray<ChatMessageContentLocal>(value).toHexString()}")
        }
    }

    private fun decode(hex: String) = BinaryScale.decodeFromByteArray<ChatMessageContentLocal>(hex.fromHex())

    private companion object {
        const val RICH_TEXT = "180001104c6f6f6b0c000024696d6167652f706e670c01020324696d6167652f706e67010300000000000000010200" +
            "0000010100000001104c4b4f32010c646f74020180000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f3c" +
            "6170706c69636174696f6e2f706466000000000014612e70646601026468747470733a2f2f6578616d706c652e636f6d2f762e6d7034" +
            "24766964656f2f6d70340000000000"
        const val ACTIONS = "180101105069636b080c7965730c596573086e6f084e6f01"
        const val DELETED = "19"
    }
}
