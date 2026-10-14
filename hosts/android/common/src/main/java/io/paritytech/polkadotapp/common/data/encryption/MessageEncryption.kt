package io.paritytech.polkadotapp.common.data.encryption

import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.utils.requireBouncyCastle
import org.bouncycastle.jce.provider.BouncyCastleProvider
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.SecretKeySpec

interface MessageEncryption {
    companion object;

    fun encrypt(plainMessage: ByteArray): ByteArray
    fun decrypt(encrypted: ByteArray): ByteArray
}

const val CHACHA20_POLY1305_NONCE_LENGTH_BYTES = 12

fun MessageEncryption.Companion.chaCha20Poly1305(key: AeadKey): MessageEncryption {
    return ChaCha20Poly1305MessageEncryption(key) {
        ByteArray(CHACHA20_POLY1305_NONCE_LENGTH_BYTES).apply { SecureRandom().nextBytes(this) }
    }
}

// The same sealed layout, with the nonce chosen by [nonceOf] from the message being sealed.
//
// Sound only when the nonce is a function of the plaintext under this key: then a repeated nonce means a
// repeated message and an identical ciphertext. A nonce reused for two different messages breaks both.
fun MessageEncryption.Companion.chaCha20Poly1305WithDerivedNonce(
    key: AeadKey,
    nonceOf: (plainMessage: ByteArray) -> ByteArray,
): MessageEncryption {
    return ChaCha20Poly1305MessageEncryption(key, nonceOf)
}

private class ChaCha20Poly1305MessageEncryption(
    key: AeadKey,
    private val nonceOf: (plainMessage: ByteArray) -> ByteArray,
) : MessageEncryption {
    companion object {
        private const val TRANSFORMATION = "CHACHA20-POLY1305"
        private const val NONCE_LENGTH_BYTES = CHACHA20_POLY1305_NONCE_LENGTH_BYTES
    }

    private val keySpec = SecretKeySpec(key.bytes.value, "ChaCha20")

    override fun encrypt(plainMessage: ByteArray): ByteArray {
        val cipher = cipher()
        val nonce = nonceOf(plainMessage)
        require(nonce.size == NONCE_LENGTH_BYTES) { "Nonce must be $NONCE_LENGTH_BYTES bytes, got ${nonce.size}" }

        cipher.init(Cipher.ENCRYPT_MODE, keySpec, IvParameterSpec(nonce))
        val cipherText = cipher.doFinal(plainMessage)

        return nonce + cipherText
    }

    override fun decrypt(encrypted: ByteArray): ByteArray {
        val cipher = cipher()
        val nonce = encrypted.copyOfRange(0, NONCE_LENGTH_BYTES)
        val cipherText = encrypted.copyOfRange(NONCE_LENGTH_BYTES, encrypted.size)

        cipher.init(Cipher.DECRYPT_MODE, keySpec, IvParameterSpec(nonce))
        return cipher.doFinal(cipherText)
    }

    private fun cipher(): Cipher {
        requireBouncyCastle()

        return Cipher.getInstance(TRANSFORMATION, BouncyCastleProvider.PROVIDER_NAME)
    }
}
