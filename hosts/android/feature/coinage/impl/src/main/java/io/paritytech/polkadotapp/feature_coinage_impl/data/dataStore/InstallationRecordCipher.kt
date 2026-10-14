package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.paritytech.polkadotapp.common.data.encryption.CHACHA20_POLY1305_NONCE_LENGTH_BYTES
import io.paritytech.polkadotapp.common.data.encryption.MessageEncryption
import io.paritytech.polkadotapp.common.data.encryption.chaCha20Poly1305
import io.paritytech.polkadotapp.common.data.encryption.chaCha20Poly1305WithDerivedNonce
import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.toCoinageInstallationId
import javax.inject.Inject

// The on-chain form of an installation id: ChaCha20-Poly1305 `nonce(12) || ciphertext(32) || tag(16)`.
// Sealing is deterministic — the nonce is a keyed hash of the id — so every attempt at registering one installation
// submits the same bytes and the contract stores it once. The hash is keyed by a subkey rather than the cipher key
// itself, keeping the two primitives on separate keys. Opening reads the nonce from the record, so records sealed
// with any nonce, e.g. random ones from another platform, open all the same.
class InstallationRecordCipher @Inject constructor() {
    fun seal(installation: CoinageInstallationId, key: AeadKey): DataByteArray {
        val encryption = MessageEncryption.chaCha20Poly1305WithDerivedNonce(key) { plain -> nonceOf(plain, key) }

        return encryption.encrypt(installation.value.value).toDataByteArray()
    }

    // Null for anything this key did not seal, which is what a record written under another seed looks like.
    fun open(record: DataByteArray, key: AeadKey): CoinageInstallationId? {
        if (record.value.size != RECORD_SIZE_BYTES) return null

        return runCatching { MessageEncryption.chaCha20Poly1305(key).decrypt(record.value) }
            .getOrNull()
            ?.takeIf { it.size == CoinageInstallationId.SIZE_BYTES }
            ?.toCoinageInstallationId()
    }

    private fun nonceOf(installation: ByteArray, key: AeadKey): ByteArray {
        val nonceKey = NONCE_KEY_CONTEXT.blake2b256(key = key.bytes.value)

        return (NONCE_CONTEXT + installation)
            .blake2b256(key = nonceKey)
            .copyOf(CHACHA20_POLY1305_NONCE_LENGTH_BYTES)
    }

    private companion object {
        val NONCE_KEY_CONTEXT = "nonce-key".encodeToByteArray()
        val NONCE_CONTEXT = "nonce".encodeToByteArray()

        const val TAG_SIZE_BYTES = 16
        const val RECORD_SIZE_BYTES = CHACHA20_POLY1305_NONCE_LENGTH_BYTES + CoinageInstallationId.SIZE_BYTES + TAG_SIZE_BYTES
    }
}
