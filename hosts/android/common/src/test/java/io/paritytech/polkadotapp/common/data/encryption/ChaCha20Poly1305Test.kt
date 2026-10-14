package io.paritytech.polkadotapp.common.data.encryption

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.AeadKey
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import javax.crypto.AEADBadTagException

private const val NONCE_LENGTH_BYTES = 12
private const val TAG_LENGTH_BYTES = 16

class ChaCha20Poly1305Test {
    private val key = AeadKey.fromDerivedBytes(
        "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f".fromHex()
    )
    private val encryption = MessageEncryption.chaCha20Poly1305(key)

    @Test
    fun `round trips a message`() {
        val plaintext = "ChaCha20-Poly1305 replaces AES-256-GCM".toByteArray()

        val decrypted = encryption.decrypt(encryption.encrypt(plaintext))

        assertArrayEquals(plaintext, decrypted)
    }

    /** RFC 8439 §2.8.2 — decrypting the documented ciphertext under the documented key and nonce. */
    @Test
    fun `matches the RFC 8439 test vector`() {
        val vectorKey = AeadKey.fromDerivedBytes(
            "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f".fromHex()
        )
        val nonce = "070000004041424344454647".fromHex()
        val cipherText = (
            "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6" +
                "3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36" +
                "92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc" +
                "3ff4def08e4b7a9de576d26586cec64b6116"
            ).fromHex()
        // RFC 8439's vector uses AAD; this protocol uses none, so only the no-AAD tag is asserted.
        val tag = "6a23a4681fd59456aea1d29f82477216".fromHex()

        val decrypted = MessageEncryption.chaCha20Poly1305(vectorKey)
            .decrypt(nonce + cipherText + tag)

        assertEquals(
            "4c616469657320616e642047656e746c656d656e206f662074686520636c6173" +
                "73206f66202739393a204966204920636f756c64206f6666657220796f75206f" +
                "6e6c79206f6e652074697020666f7220746865206675747572652c2073756e73" +
                "637265656e20776f756c642062652069742e",
            decrypted.toHexString(),
        )
    }

    @Test
    fun `framing is nonce then ciphertext then tag`() {
        val plaintext = ByteArray(41) { it.toByte() }

        val encrypted = encryption.encrypt(plaintext)

        assertEquals(NONCE_LENGTH_BYTES + plaintext.size + TAG_LENGTH_BYTES, encrypted.size)
    }

    @Test
    fun `each encryption uses a fresh nonce`() {
        val plaintext = "same message".toByteArray()

        val first = encryption.encrypt(plaintext).copyOfRange(0, NONCE_LENGTH_BYTES)
        val second = encryption.encrypt(plaintext).copyOfRange(0, NONCE_LENGTH_BYTES)

        assertNotEquals(first.toHexString(), second.toHexString())
    }

    @Test(expected = AEADBadTagException::class)
    fun `bit flipped ciphertext fails authentication`() {
        val encrypted = encryption.encrypt("authentic".toByteArray())
        encrypted[NONCE_LENGTH_BYTES] = (encrypted[NONCE_LENGTH_BYTES].toInt() xor 0x01).toByte()

        encryption.decrypt(encrypted)
    }

    @Test(expected = AEADBadTagException::class)
    fun `truncated ciphertext fails authentication`() {
        val encrypted = encryption.encrypt("authentic".toByteArray())

        encryption.decrypt(encrypted.copyOfRange(0, encrypted.size - 1))
    }
}
