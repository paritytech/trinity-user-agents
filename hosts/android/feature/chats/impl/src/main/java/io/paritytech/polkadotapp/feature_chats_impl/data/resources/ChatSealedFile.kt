package io.paritytech.polkadotapp.feature_chats_impl.data.resources

import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.common.utils.requireBouncyCastle
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import java.io.ByteArrayOutputStream
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.SecretKeySpec

// XChaCha20-Poly1305 of the core (`SealedBytes`), which the JCE does not have: HChaCha20 gives the subkey, and
// ChaCha20-Poly1305 runs under it with the last 8 bytes of the nonce.
object ChatSealedFile {
    private val ASSOCIATED_DATA = "truapi/chat/sealed/v1".encodeToByteArray()
    private const val KEY_BYTES = 32
    private const val NONCE_BYTES = 24
    private const val SUBKEY_NONCE_BYTES = 16
    private const val TRANSFORMATION = "CHACHA20-POLY1305"
    private const val PROVIDER = "BC"

    suspend fun read(resource: ChatResourceModel.Sealed, lookup: suspend (ByteArray) -> ByteArray?): ByteArray? {
        val ciphertext = ByteArrayOutputStream()
        for (part in resource.parts) {
            val value = lookup(part.value) ?: return null
            if (!value.blake2b256().contentEquals(part.value)) return null
            ciphertext.write(value)
            if (ciphertext.size() > resource.size) return null
        }
        return open(resource, ciphertext.toByteArray())
    }

    fun open(resource: ChatResourceModel.Sealed, ciphertext: ByteArray): ByteArray? {
        val key = resource.key.value
        val nonce = resource.nonce.value
        if (ciphertext.size.toLong() != resource.size || key.size != KEY_BYTES || nonce.size != NONCE_BYTES) return null
        if (!MessageDigest.getInstance("SHA-512").digest(ciphertext).contentEquals(resource.digest.value)) return null

        requireBouncyCastle()
        val subkey = HChaCha20.subkey(key, nonce.copyOfRange(0, SUBKEY_NONCE_BYTES))
        val chachaNonce = ByteArray(4) + nonce.copyOfRange(SUBKEY_NONCE_BYTES, NONCE_BYTES)
        return runCatching {
            Cipher.getInstance(TRANSFORMATION, PROVIDER).run {
                init(Cipher.DECRYPT_MODE, SecretKeySpec(subkey, "ChaCha20"), IvParameterSpec(chachaNonce))
                updateAAD(ASSOCIATED_DATA)
                doFinal(ciphertext)
            }
        }.getOrNull()
    }
}

object HChaCha20 {
    private val CONSTANTS = intArrayOf(0x61707865, 0x3320646e, 0x79622d32, 0x6b206574)
    private const val DOUBLE_ROUNDS = 10

    fun subkey(key: ByteArray, nonce: ByteArray): ByteArray {
        val state = CONSTANTS + words(key, 8) + words(nonce, 4)
        repeat(DOUBLE_ROUNDS) {
            quarterRound(state, 0, 4, 8, 12)
            quarterRound(state, 1, 5, 9, 13)
            quarterRound(state, 2, 6, 10, 14)
            quarterRound(state, 3, 7, 11, 15)
            quarterRound(state, 0, 5, 10, 15)
            quarterRound(state, 1, 6, 11, 12)
            quarterRound(state, 2, 7, 8, 13)
            quarterRound(state, 3, 4, 9, 14)
        }
        val output = state.sliceArray(0 until 4) + state.sliceArray(12 until 16)
        return ByteArray(output.size * 4) { index -> (output[index / 4] ushr (8 * (index % 4))).toByte() }
    }

    private fun words(bytes: ByteArray, count: Int) = IntArray(count) { word ->
        (0 until 4).fold(0) { value, shift -> value or ((bytes[word * 4 + shift].toInt() and 0xff) shl (8 * shift)) }
    }

    private fun quarterRound(state: IntArray, first: Int, second: Int, third: Int, fourth: Int) {
        state[first] += state[second]
        state[fourth] = (state[fourth] xor state[first]).rotateLeft(16)
        state[third] += state[fourth]
        state[second] = (state[second] xor state[third]).rotateLeft(12)
        state[first] += state[second]
        state[fourth] = (state[fourth] xor state[first]).rotateLeft(8)
        state[third] += state[fourth]
        state[second] = (state[second] xor state[third]).rotateLeft(7)
    }
}
