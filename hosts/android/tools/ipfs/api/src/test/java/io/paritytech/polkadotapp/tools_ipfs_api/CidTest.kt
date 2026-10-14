package io.paritytech.polkadotapp.tools_ipfs_api

import io.ipfs.multihash.Multihash
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.ByteArrayInputStream

class CidTest {
    // -- cast(byte[]) --

    @Test
    fun `cast byte array parses CIDv0`() {
        val cid = createCidV0()
        val bytes = cid.toBytes()

        val parsed = Cid.cast(bytes)

        assertEquals(0L, parsed.version)
        assertEquals(Cid.Codec.DagProtobuf, parsed.codec)
        assertEquals(Multihash.Type.sha2_256, parsed.type)
        assertArrayEquals(cid.hash, parsed.hash)
    }

    @Test
    fun `cast byte array parses CIDv1 DagProtobuf`() {
        val cid = Cid.buildCidV1(Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, fakeHash("v1-dagpb"))

        val parsed = Cid.cast(cid.toBytes())

        assertEquals(1L, parsed.version)
        assertEquals(Cid.Codec.DagProtobuf, parsed.codec)
        assertArrayEquals(cid.hash, parsed.hash)
    }

    @Test
    fun `cast byte array parses CIDv1 Raw codec`() {
        val cid = Cid.buildCidV1(Cid.Codec.Raw, Multihash.Type.sha2_256, fakeHash("v1-raw"))

        val parsed = Cid.cast(cid.toBytes())

        assertEquals(1L, parsed.version)
        assertEquals(Cid.Codec.Raw, parsed.codec)
        assertArrayEquals(cid.hash, parsed.hash)
    }

    @Test
    fun `cast byte array parses CIDv1 DagCbor codec`() {
        val cid = Cid.buildCidV1(Cid.Codec.DagCbor, Multihash.Type.sha2_256, fakeHash("v1-dagcbor"))

        val parsed = Cid.cast(cid.toBytes())

        assertEquals(1L, parsed.version)
        assertEquals(Cid.Codec.DagCbor, parsed.codec)
    }

    @Test
    fun `cast byte array round-trips CIDv0`() {
        val original = createCidV0()
        val parsed = Cid.cast(original.toBytes())

        assertArrayEquals(original.toBytes(), parsed.toBytes())
    }

    @Test
    fun `cast byte array round-trips CIDv1`() {
        val original = Cid.buildCidV1(Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, fakeHash("roundtrip"))
        val parsed = Cid.cast(original.toBytes())

        assertArrayEquals(original.toBytes(), parsed.toBytes())
    }

    @Test(expected = Cid.CidEncodingException::class)
    fun `cast byte array rejects invalid version`() {
        // Version 5, some codec, then garbage — should fail
        Cid.cast(byteArrayOf(5, 0x70, 0x12, 0x20) + ByteArray(32))
    }

    @Test(expected = Cid.CidEncodingException::class)
    fun `cast byte array rejects empty input`() {
        Cid.cast(ByteArray(0))
    }

    // -- cast(InputStream) --

    @Test
    fun `cast stream parses CIDv0 and leaves remainder`() {
        val cid = createCidV0()
        val trailing = "trailing data".toByteArray()
        val stream = ByteArrayInputStream(cid.toBytes() + trailing)

        val parsed = Cid.cast(stream)

        assertEquals(0L, parsed.version)
        assertEquals(Cid.Codec.DagProtobuf, parsed.codec)
        assertArrayEquals(cid.hash, parsed.hash)
        // Stream should have the trailing data remaining
        assertArrayEquals(trailing, stream.readBytes())
    }

    @Test
    fun `cast stream parses CIDv1 and leaves remainder`() {
        val cid = Cid.buildCidV1(Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, fakeHash("stream-v1"))
        val trailing = "more data".toByteArray()
        val stream = ByteArrayInputStream(cid.toBytes() + trailing)

        val parsed = Cid.cast(stream)

        assertEquals(1L, parsed.version)
        assertEquals(Cid.Codec.DagProtobuf, parsed.codec)
        assertArrayEquals(cid.hash, parsed.hash)
        assertArrayEquals(trailing, stream.readBytes())
    }

    @Test
    fun `cast stream parses CIDv0 with no trailing data`() {
        val cid = createCidV0()
        val stream = ByteArrayInputStream(cid.toBytes())

        val parsed = Cid.cast(stream)

        assertEquals(0L, parsed.version)
        assertEquals(0, stream.available())
    }

    @Test
    fun `cast stream parses CIDv1 with no trailing data`() {
        val cid = Cid.buildCidV1(Cid.Codec.Raw, Multihash.Type.sha2_256, fakeHash("no-trail"))
        val stream = ByteArrayInputStream(cid.toBytes())

        val parsed = Cid.cast(stream)

        assertEquals(1L, parsed.version)
        assertEquals(Cid.Codec.Raw, parsed.codec)
        assertEquals(0, stream.available())
    }

    @Test
    fun `cast stream handles sequential CIDs`() {
        val cid1 = createCidV0()
        val cid2 = Cid.buildCidV1(Cid.Codec.Raw, Multihash.Type.sha2_256, fakeHash("second"))
        val stream = ByteArrayInputStream(cid1.toBytes() + cid2.toBytes())

        val parsed1 = Cid.cast(stream)
        val parsed2 = Cid.cast(stream)

        assertEquals(0L, parsed1.version)
        assertEquals(1L, parsed2.version)
        assertEquals(Cid.Codec.Raw, parsed2.codec)
        assertEquals(0, stream.available())
    }

    @Test
    fun `cast stream and cast byte array produce same result for CIDv0`() {
        val cid = createCidV0()
        val bytes = cid.toBytes()

        val fromBytes = Cid.cast(bytes)
        val fromStream = Cid.cast(ByteArrayInputStream(bytes))

        assertEquals(fromBytes.version, fromStream.version)
        assertEquals(fromBytes.codec, fromStream.codec)
        assertArrayEquals(fromBytes.hash, fromStream.hash)
    }

    @Test
    fun `cast stream and cast byte array produce same result for CIDv1`() {
        val cid = Cid.buildCidV1(Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, fakeHash("consistency"))
        val bytes = cid.toBytes()

        val fromBytes = Cid.cast(bytes)
        val fromStream = Cid.cast(ByteArrayInputStream(bytes))

        assertEquals(fromBytes.version, fromStream.version)
        assertEquals(fromBytes.codec, fromStream.codec)
        assertArrayEquals(fromBytes.hash, fromStream.hash)
    }

    // -- Helpers --

    private fun createCidV0(): Cid {
        return Cid(0, Cid.Codec.DagProtobuf, Multihash.Type.sha2_256, fakeHash("v0"))
    }

    private fun fakeHash(key: String): ByteArray {
        return key.encodeToByteArray().copyOf(32)
    }
}
