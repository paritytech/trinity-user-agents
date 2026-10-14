package io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption

import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.x25519SharedSecret
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.SharedSecretDerivationUseCase
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import javax.inject.Inject

class CommunicationEncryptionFactory @Inject constructor(
    private val sharedSecretDerivationUseCase: SharedSecretDerivationUseCase,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
) : CommunicationEncryption.Factory {
    override suspend fun createWithDeviceKeypair(peerPublicKey: X25519PublicKey): CommunicationEncryption {
        return createEncryption(
            localKeypair = ourDeviceKeypairProvider.get(),
            peerPublicKey = peerPublicKey,
        )
    }

    override suspend fun create(
        domain: SharedSecretDerivationDomain,
        peerPublicKey: X25519PublicKey,
    ): CommunicationEncryption {
        return createEncryption(
            localKeypair = sharedSecretDerivationUseCase.deriveForDomain(domain),
            peerPublicKey = peerPublicKey
        )
    }

    override suspend fun createOneTimeUse(peerPublicKey: X25519PublicKey): CommunicationEncryption {
        return createEncryption(
            localKeypair = sharedSecretDerivationUseCase.generateOneTimeUse(),
            peerPublicKey = peerPublicKey
        )
    }

    override suspend fun createEncryption(
        localKeypair: X25519KeyPair,
        peerPublicKey: X25519PublicKey,
    ): CommunicationEncryption {
        // A small-order peer key aborts the agreement here, rejecting the enclosing message.
        val sharedSecret = x25519SharedSecret(localKeypair.privateKey, peerPublicKey).getOrThrow()

        return RealCommunicationEncryption(
            localPublicKey = localKeypair.publicKey,
            sharedSecret = sharedSecret,
        )
    }
}
