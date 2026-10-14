package io.paritytech.polkadotapp.common.utils.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class BigEndianU32ScaleTest {
    @Test
    fun `encodes in big endian`() {
        val encoded = encode(0x01020304u)
        assertArrayEquals(byteArrayOf(0x01, 0x02, 0x03, 0x04), encoded)
    }

    @Test
    fun `decodes from big endian`() {
        val bytes = byteArrayOf(0x01, 0x02, 0x03, 0x04)
        val decoded = decode(bytes)
        assertEquals(0x01020304u, decoded.value)
    }

    @Test
    fun `encoded length is always 4 bytes`() {
        listOf(0u, 1u, 255u, 256u, 65535u, 65536u, UInt.MAX_VALUE).forEach { v ->
            assertEquals("u=$v", 4, encode(v).size)
        }
    }

    @Test
    fun roundtrip() {
        listOf(0u, 1u, 255u, 256u, 65535u, UInt.MAX_VALUE).forEach { v ->
            val encoded = encode(v)
            val decoded = decode(encoded)
            assertEquals(v, decoded.value)
        }
    }

    private fun encode(value: UInt): ByteArray = Scale.encode(BigEndianU32Scale(value)) as ByteArray

    private fun decode(bytes: ByteArray): BigEndianU32Scale = Scale.decode(bytes)
}
