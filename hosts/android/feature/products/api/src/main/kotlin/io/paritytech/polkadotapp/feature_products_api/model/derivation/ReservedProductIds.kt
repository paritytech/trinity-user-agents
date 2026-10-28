package io.paritytech.polkadotapp.feature_products_api.model.derivation

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import uniffi.truapi.ProductId

/**
 * RFC-0022 governance-reserved dotNS identities for built-in app features.
 *
 * Reserved names live in each network's own namespace (`dim2.dot` on Polkadot, `dim2.paseo`
 * on Paseo), so every id is a function of the network TLD — matching the runtime's
 * network-specific product providers (individuality#1300).
 */
object ReservedProductIds {
    fun game(tld: DotNsTld): ProductId = ProductId.fromName("dim2", tld.value).getOrThrow()

    fun funding(tld: DotNsTld): ProductId = ProductId.fromName("fund", tld.value).getOrThrow()

    fun lightPersonIdentity(tld: DotNsTld): ProductId = ProductId.fromName("uid", tld.value).getOrThrow()

    fun personhood(tld: DotNsTld): ProductId = ProductId.fromName("peopl", tld.value).getOrThrow()
}
