package io.paritytech.polkadotapp.feature_statement_store_api.data.encryption

import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.X25519SharedSecret
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain

interface CommunicationEncryption {
    interface Factory {
        suspend fun create(
            domain: SharedSecretDerivationDomain,
            peerPublicKey: X25519PublicKey,
        ): CommunicationEncryption

        suspend fun createOneTimeUse(
            peerPublicKey: X25519PublicKey,
        ): CommunicationEncryption

        /**
         * Builds a [CommunicationEncryption] with the provided [localKeypair] as the local
         * side. Used when the local side is something other than a wallet-seed-derived
         * keypair — e.g. inter-own-device sync uses the device's own X25519 keypair from
         * [io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider].
         */
        suspend fun createEncryption(
            localKeypair: X25519KeyPair,
            peerPublicKey: X25519PublicKey,
        ): CommunicationEncryption

        suspend fun createWithDeviceKeypair(
            peerPublicKey: X25519PublicKey,
        ): CommunicationEncryption
    }

    val sharedSecret: X25519SharedSecret

    val localPublicKey: X25519PublicKey

    fun encrypt(message: ByteArray): ByteArray

    fun decrypt(encryptedMessage: ByteArray): ByteArray
}
