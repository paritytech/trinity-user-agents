package io.paritytech.polkadotapp.feature_account_api.domain.derivation

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy

/**
 * Derives ring-VRF entropy at an arbitrary keyed-hash path of the RFC-0022 ring-VRF tree.
 *
 * Every meta account is created from the same mnemonic, so the tree has a single root and a path is
 * enough to name a key — no account has to be supplied.
 */
interface RingVrfEntropyDeriver {
    suspend fun deriveRingVrfEntropy(path: String): BandersnatchEntropy
}
