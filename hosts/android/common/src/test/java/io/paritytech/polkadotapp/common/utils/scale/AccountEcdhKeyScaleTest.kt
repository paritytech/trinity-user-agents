package io.paritytech.polkadotapp.common.utils.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.paritytech.polkadotapp.common.domain.model.AccountEcdhKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.domain.model.x25519OrNull
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AccountEcdhKeyScaleTest {
    private val keyBytes = ByteArray(X25519PublicKey.SIZE_BYTES) { 0xAB.toByte() }
    private val key = X25519PublicKey.fromBytes(keyBytes.toDataByteArray()).getOrThrow()

    @Test
    fun `x25519 key encodes as type byte then key then zero padding`() {
        val encoded = encode(AccountEcdhKey.X25519(key))

        assertEquals(AccountEcdhKeyScaleSerializer.CONTAINER_SIZE_BYTES, encoded.size)
        assertEquals(0x00.toByte(), encoded[0])
        assertArrayEquals(keyBytes, encoded.copyOfRange(1, 33))
        assertArrayEquals(ByteArray(32), encoded.copyOfRange(33, 65))
    }

    @Test
    fun `x25519 key round trips to domain`() {
        val decoded = decode(encode(AccountEcdhKey.X25519(key)))

        assertEquals(key, decoded.toDomain().getOrThrow().x25519OrNull())
    }

    /** A future curve must not break reads of an account that merely happens to use it. */
    @Test
    fun `unrecognised type byte decodes as Unknown`() {
        val raw = unknownContainer()

        val domain = decode(raw).toDomain().getOrThrow()

        assertNull(domain.x25519OrNull())
        assertArrayEquals(raw, (domain as AccountEcdhKey.Unknown).rawValue.value)
    }

    @Test
    fun `unknown key re-encodes byte for byte`() {
        val raw = unknownContainer()

        assertArrayEquals(raw, encode(AccountEcdhKey.Unknown(raw.toDataByteArray())))
    }

    @Test
    fun `padding is ignored on read`() {
        val padded = encode(AccountEcdhKey.X25519(key)).also { it.fill(0x7F, 33, 65) }

        assertEquals(key, decode(padded).toDomain().getOrThrow().x25519OrNull())
    }

    private fun unknownContainer(): ByteArray =
        ByteArray(AccountEcdhKeyScaleSerializer.CONTAINER_SIZE_BYTES) { 0x11 }.also { it[0] = 0x04 }

    private fun encode(key: AccountEcdhKey): ByteArray = Scale.encode(key.toScale()) as ByteArray

    private fun decode(bytes: ByteArray): AccountEcdhKeyScale = Scale.decode(bytes)
}
