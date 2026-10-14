package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

/**
 * Lazily-generated, persisted X25519 keypair identifying this device for multi-device
 * envelope encryption. Distinct from the identity chat keypair.
 */
interface OurDeviceKeypairProvider {
    suspend fun get(): X25519KeyPair

    suspend fun publicKey(): X25519PublicKey
}
