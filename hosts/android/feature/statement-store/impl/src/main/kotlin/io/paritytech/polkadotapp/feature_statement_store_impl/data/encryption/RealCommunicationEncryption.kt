package io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption

import io.paritytech.polkadotapp.common.data.encryption.MessageEncryption
import io.paritytech.polkadotapp.common.data.encryption.chaCha20Poly1305
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.X25519SharedSecret
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption

internal class RealCommunicationEncryption(
    override val localPublicKey: X25519PublicKey,
    override val sharedSecret: X25519SharedSecret,
) : CommunicationEncryption {
    private val messageEncryption: MessageEncryption =
        MessageEncryption.chaCha20Poly1305(hkdfSha256(sharedSecret))

    override fun encrypt(message: ByteArray): ByteArray = messageEncryption.encrypt(message)

    override fun decrypt(encryptedMessage: ByteArray): ByteArray = messageEncryption.decrypt(encryptedMessage)
}
