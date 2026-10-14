package io.paritytech.polkadotapp.feature_account_api.domain.derivation

/**
 * Supplies the keyed-hash path of an account's ring-VRF entropy.
 *
 * Contributed only for the purposes that have a ring-VRF key; an absent entry means the purpose has none.
 */
interface RingVrfDerivationProvider {
    suspend fun provideDerivationPath(): String
}
