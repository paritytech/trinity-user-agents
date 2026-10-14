package io.paritytech.polkadotapp.common.utils

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class X25519CryptoTest {
    private val keyGenerator = X25519KeyGenerator()

    /** RFC 7748 §6.1 — Alice's private key and the public key it must produce. */
    @Test
    fun `public key matches the RFC 7748 test vector`() {
        val privateKey = privateKey("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")

        val keyPair = keyGenerator.createKeyPair(privateKey)

        assertEquals(
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a",
            keyPair.publicKey.bytes.value.toHexString(),
        )
    }

    /** RFC 7748 §6.1 — Alice and Bob must reach the documented shared secret. */
    @Test
    fun `shared secret matches the RFC 7748 test vector`() {
        val alicePrivate = privateKey("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
        val bobPublic = publicKey("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")

        val secret = x25519SharedSecret(alicePrivate, bobPublic).getOrThrow()

        assertEquals(
            "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742",
            secret.bytes.value.toHexString(),
        )
    }

    @Test
    fun `both peers derive the same secret`() {
        val alice = keyGenerator.generateRandomKeypair()
        val bob = keyGenerator.generateRandomKeypair()

        val fromAlice = x25519SharedSecret(alice.privateKey, bob.publicKey).getOrThrow()
        val fromBob = x25519SharedSecret(bob.privateKey, alice.publicKey).getOrThrow()

        assertEquals(fromAlice, fromBob)
    }

    /**
     * RFC-0004 makes this normative: a small-order public key drives the agreement to all zeros,
     * and the operation must abort rather than proceed with a degenerate secret.
     */
    @Test
    fun `small order public key aborts the agreement`() {
        val local = keyGenerator.generateRandomKeypair()

        SMALL_ORDER_PUBLIC_KEYS.forEach { hex ->
            val result = x25519SharedSecret(local.privateKey, publicKey(hex))

            assertTrue("expected failure for small-order key $hex", result.isFailure)
        }
    }

    @Test
    fun `key rejects input that is not 32 bytes`() {
        assertTrue(X25519PublicKey.fromBytes(byteArrayOf(1, 2, 3).toDataByteArray()).isFailure)
        assertTrue(X25519PrivateKey.fromBytes(ByteArray(33).toDataByteArray()).isFailure)
    }

    private fun privateKey(hex: String) = X25519PrivateKey.fromBytes(hex.fromHex().toDataByteArray()).getOrThrow()

    private fun publicKey(hex: String) = X25519PublicKey.fromBytes(hex.fromHex().toDataByteArray()).getOrThrow()

    private companion object {
        // RFC 7748 §6.1 / the canonical libsodium blacklist: points of order 1, 2, 4 and 8.
        val SMALL_ORDER_PUBLIC_KEYS = listOf(
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0100000000000000000000000000000000000000000000000000000000000000",
            "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
            "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f1157",
        )
    }
}
