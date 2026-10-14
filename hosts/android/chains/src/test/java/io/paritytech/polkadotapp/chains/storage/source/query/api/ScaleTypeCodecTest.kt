package io.paritytech.polkadotapp.chains.storage.source.query.api

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import kotlinx.serialization.Serializable
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.reflect.typeOf

class ScaleTypeCodecTest {
    @Serializable
    private data class Position(val ringIndex: Int, val ringPosition: Int)

    private val position = Position(ringIndex = 7, ringPosition = 3)
    private val codec = ScaleTypeCodec<Position>(typeOf<Position>())

    @Test
    fun `decodes what the type-based lookup encodes`() {
        val instance = Scale.encode(typeOf<Position>(), position)

        assertEquals(position, codec.decode(instance))
    }

    @Test
    fun `encodes what the type-based lookup decodes`() {
        val instance = codec.encode(position)

        assertEquals(position, Scale.decode<Position>(typeOf<Position>(), instance))
    }

    @Test
    fun `byte arrays keep the dynamic-struct serializer`() {
        val bytes = byteArrayOf(1, 2, 3)
        val bytesCodec = ScaleTypeCodec<ByteArray>(typeOf<ByteArray>())

        assertArrayEquals(bytes, bytesCodec.encode(bytes) as ByteArray)
        assertArrayEquals(bytes, bytesCodec.decode(bytes))
    }
}
