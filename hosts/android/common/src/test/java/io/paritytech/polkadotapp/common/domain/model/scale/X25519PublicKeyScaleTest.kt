package io.paritytech.polkadotapp.common.domain.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.decodeFromByteArrayCatching
import io.paritytech.polkadotapp.common.utils.encodeToByteArrayCatching
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class X25519PublicKeyScaleTest {
    private val key = ByteArray(X25519PublicKey.SIZE_BYTES) { 0xAB.toByte() }

    /**
     * The whole "one wrapper instead of seven @FixedLength annotations" decision rests on this:
     * a value class must not gain a length prefix when the encoder descends into it.
     */
    @Test
    fun `encodes to 32 raw bytes with no length prefix`() {
        val encoded = encode(X25519PublicKeyScale(key))

        assertEquals(X25519PublicKey.SIZE_BYTES, encoded.size)
        assertArrayEquals(key, encoded)
    }

    @Test
    fun `round trips through scale`() {
        val encoded = encode(X25519PublicKeyScale(key))

        val decoded = BinaryScale.decodeFromByteArrayCatching<X25519PublicKeyScale>(encoded).getOrThrow()

        assertArrayEquals(key, decoded.bytes)
    }

    @Test
    fun `domain mapping round trips`() {
        val domain = X25519PublicKey.fromBytes(key.toDataByteArray()).getOrThrow()

        assertEquals(domain, domain.toScale().toDomain().getOrThrow())
    }

    private fun encode(value: X25519PublicKeyScale): ByteArray =
        BinaryScale.encodeToByteArrayCatching(value).getOrThrow()
}
