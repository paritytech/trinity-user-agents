package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopPoolEntryPayload
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.VersionedHopPoolEntry
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatementContent
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.NodeEndpoint
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Cross-platform conformance vectors for the compaction wire format. The hex values are frozen:
 * they pin the byte layout the iOS client produces/expects (RFC "Message Compaction"), so any
 * codec drift fails here instead of as an undecodable message on the peer.
 */
class CompactionScaleConformanceTest {
    private val claimIdentifier = ByteArray(32) { it.toByte() }
    private val claimTicket = ByteArray(32) { (it + 0x20).toByte() }

    private val commitVectorHex =
        "13" +
            "80" + "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f" +
            "80" + "202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f" +
            "00" + "587773733a2f2f686f702e6578616d706c652f6e6f6465"

    // Vec of two opaque encoded messages: [aa bb cc] and [01 02]
    private val batchVectorHex = "080caabbcc080102"

    private val envelopeVectorHex = "0000" + "20" + batchVectorHex

    @Test
    fun `CompactionCommit encodes to the frozen vector`() {
        val commit: ChatMessageStatementContent = ChatMessageStatementContent.CompactionCommit(
            claimIdentifier = claimIdentifier,
            claimTicket = claimTicket,
            node = NodeEndpoint.WssUrl("wss://hop.example/node")
        )

        val encoded = BinaryScale.encodeToByteArray(commit)

        assertEquals(commitVectorHex, encoded.toHexString())
    }

    @Test
    fun `CompactionCommit decodes from the frozen vector`() {
        val decoded = BinaryScale.decodeFromByteArray<ChatMessageStatementContent>(commitVectorHex.fromHex())

        assertTrue(decoded is ChatMessageStatementContent.CompactionCommit)
        val commit = decoded as ChatMessageStatementContent.CompactionCommit
        assertArrayEquals(claimIdentifier, commit.claimIdentifier)
        assertArrayEquals(claimTicket, commit.claimTicket)
        assertEquals("wss://hop.example/node", (commit.node as NodeEndpoint.WssUrl).url)
    }

    @Test
    fun `batch of opaque messages encodes as a plain SCALE vector`() {
        val batch = listOf(byteArrayOf(0xAA.toByte(), 0xBB.toByte(), 0xCC.toByte()), byteArrayOf(0x01, 0x02))

        val encoded = BinaryScale.encodeToByteArray(batch)

        assertEquals(batchVectorHex, encoded.toHexString())
    }

    @Test
    fun `inline envelope with batch payload encodes to the frozen vector`() {
        val envelope: VersionedHopPoolEntry = VersionedHopPoolEntry.V1(
            HopPoolEntryPayload.Inline(batchVectorHex.fromHex())
        )

        val encoded = BinaryScale.encodeToByteArray(envelope)

        assertEquals(envelopeVectorHex, encoded.toHexString())
    }

    @Test
    fun `inline envelope with batch payload decodes back to the original messages`() {
        val decoded = BinaryScale.decodeFromByteArray<VersionedHopPoolEntry>(envelopeVectorHex.fromHex())

        val inline = (decoded as VersionedHopPoolEntry.V1).payload as HopPoolEntryPayload.Inline
        val messages = BinaryScale.decodeFromByteArray<List<ByteArray>>(inline.bytes)

        assertEquals(2, messages.size)
        assertArrayEquals(byteArrayOf(0xAA.toByte(), 0xBB.toByte(), 0xCC.toByte()), messages[0])
        assertArrayEquals(byteArrayOf(0x01, 0x02), messages[1])
    }

    @Test
    fun `unknown envelope version fails to decode`() {
        val unknownVersion = "01" + "00" + "20" + batchVectorHex

        val result = runCatching { BinaryScale.decodeFromByteArray<VersionedHopPoolEntry>(unknownVersion.fromHex()) }

        assertTrue(result.isFailure)
    }
}
