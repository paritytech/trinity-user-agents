package io.paritytech.polkadotapp.feature_account_api.domain.model

/**
 * RFC-0022 ECDH derivation domain. Holds the domain name itself, not a derivation path — the ECDH
 * tree is rooted directly in the account entropy rather than in the sr25519 keypair.
 */
@JvmInline
value class SharedSecretDerivationDomain(val domain: String) {
    companion object {
        val CHAT = SharedSecretDerivationDomain("chat")
    }
}
