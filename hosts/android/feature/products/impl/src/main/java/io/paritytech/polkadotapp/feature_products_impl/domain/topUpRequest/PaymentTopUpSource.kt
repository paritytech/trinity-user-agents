package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.common.domain.model.EncodedPrivateKey
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32

/**
 * Where the funds for a RFC-0006 top-up come from. Mirrors the `PaymentTopUpSource` enum from the
 * host API protocol: `ProductAccount` carries only a derivation index — the product id is always
 * the calling product (enforced at the protocol level, so products cannot top-up from another
 * product's accounts).
 */
sealed interface PaymentTopUpSource {
    data class ProductAccount(val index: DerivationIndex32) : PaymentTopUpSource

    data class PrivateKey(val key: EncodedPrivateKey) : PaymentTopUpSource

    /**
     * A set of coins to be moved into the user's own coin set, identified by their sr25519 secret
     * keys (e.g. coins received via a W3S real-time payment).
     */
    data class Coins(val secretKeys: List<EncodedPrivateKey>) : PaymentTopUpSource
}
