package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

/**
 * How much of a registry entry the caller is asking for. A member public key is linkable across
 * every ring it appears in, which is why disclosing it is a separate ask.
 */
enum class RingVrfKeyDisclosure {
    ANONYMIZED,
    PUBLIC_KEY,
}
