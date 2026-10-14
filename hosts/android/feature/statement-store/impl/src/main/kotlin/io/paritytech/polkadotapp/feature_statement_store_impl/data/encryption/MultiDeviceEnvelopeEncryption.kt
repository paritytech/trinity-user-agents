package io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption

import io.paritytech.polkadotapp.common.data.encryption.MessageEncryption
import io.paritytech.polkadotapp.common.data.encryption.chaCha20Poly1305
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.x25519SharedSecret
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.RequestDeviceInfo
import javax.inject.Inject

/**
 * Wraps/unwraps chat payloads addressed to multiple recipient devices of one peer.
 *
 * A fresh 256-bit key (PK) encrypts the payload; PK is then re-encrypted for each recipient device
 * with a per-device key derived from X25519 (via [deriveSharedSecretWith]) × that device's
 * encryption public key. Receivers look up their own entry in `devicesInfo` by statement account id
 * and reverse the flow.
 *
 * Bound to "our device" ([ourStatementAccountId]); the private half is hidden behind
 * [deriveSharedSecretWith]. Build via [Factory] per session.
 */
class MultiDeviceEnvelopeEncryption(
    private val ourStatementAccountId: AccountId,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider
) {
    class Factory @Inject constructor(
        private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    ) {
        fun create(ourStatementAccountId: AccountId): MultiDeviceEnvelopeEncryption = MultiDeviceEnvelopeEncryption(
            ourStatementAccountId = ourStatementAccountId,
            ourDeviceKeypairProvider = ourDeviceKeypairProvider
        )
    }

    data class Recipient(
        val statementAccountId: AccountId,
        val encryptionPublicKey: X25519PublicKey,
    )

    data class WrappedPayload(
        val encryptedPayload: DataByteArray,
        val devicesInfo: List<RequestDeviceInfo>,
    )

    /** @throws IllegalArgumentException if [recipients] is empty. */
    suspend fun wrap(
        payload: ByteArray,
        recipients: List<Recipient>,
    ): WrappedPayload {
        require(recipients.isNotEmpty()) {
            "Cannot wrap multi-device payload with no recipient devices"
        }

        val symmetricKey = AeadKey.random()
        val payloadEncryption = MessageEncryption.chaCha20Poly1305(symmetricKey)
        val encryptedPayload = payloadEncryption.encrypt(payload)

        val devicesInfo = recipients.map { recipient ->
            val perDeviceKey = deriveEnvelopeKey(recipient.encryptionPublicKey)
            val encryptedKey = MessageEncryption.chaCha20Poly1305(perDeviceKey).encrypt(symmetricKey.bytes.value)

            RequestDeviceInfo(
                statementAccountId = recipient.statementAccountId.value,
                encryptedKey = encryptedKey.toDataByteArray(),
            )
        }

        return WrappedPayload(encryptedPayload.toDataByteArray(), devicesInfo)
    }

    suspend fun unwrap(
        encryptedPayload: ByteArray,
        devicesInfo: List<RequestDeviceInfo>,
        senderEncryptionPublicKey: X25519PublicKey,
    ): ByteArray {
        val ownEntry = devicesInfo.firstOrNull { it.statementAccountId.toDataByteArray() == ourStatementAccountId }
            ?: error("Multi-device envelope is not addressed to this device")

        val perDeviceKey = deriveEnvelopeKey(senderEncryptionPublicKey)
        val symmetricKey = MessageEncryption.chaCha20Poly1305(perDeviceKey).decrypt(ownEntry.encryptedKey.value)

        return MessageEncryption.chaCha20Poly1305(AeadKey.fromDerivedBytes(symmetricKey)).decrypt(encryptedPayload)
    }

    suspend fun unwrapOwn(
        encryptedPayload: ByteArray,
        devicesInfo: List<RequestDeviceInfo>,
        peerDevices: List<Recipient>,
    ): ByteArray {
        val peersByAccount = peerDevices.associateBy { it.statementAccountId }
        val entry = devicesInfo.firstOrNull { peersByAccount.containsKey(it.statementAccountId.toDataByteArray()) }
            ?: error("Own multi-device envelope has no known peer device entry")

        val peerPublicKey = peersByAccount.getValue(entry.statementAccountId.toDataByteArray()).encryptionPublicKey
        val perDeviceKey = deriveEnvelopeKey(peerPublicKey)
        val symmetricKey = MessageEncryption.chaCha20Poly1305(perDeviceKey).decrypt(entry.encryptedKey.value)

        return MessageEncryption.chaCha20Poly1305(AeadKey.fromDerivedBytes(symmetricKey)).decrypt(encryptedPayload)
    }

    private suspend fun deriveEnvelopeKey(peerPublicKey: X25519PublicKey): AeadKey {
        return hkdfSha256(deriveSharedSecretWith(peerPublicKey))
    }

    private suspend fun deriveSharedSecretWith(publicKey: X25519PublicKey) =
        x25519SharedSecret(ourDeviceKeypairProvider.get().privateKey, publicKey).getOrThrow()
}

fun List<DeviceInfo>.toEnvelopeRecipients(): List<MultiDeviceEnvelopeEncryption.Recipient> = map {
    MultiDeviceEnvelopeEncryption.Recipient(
        statementAccountId = it.statementAccountId,
        encryptionPublicKey = it.encryptionPublicKey,
    )
}
