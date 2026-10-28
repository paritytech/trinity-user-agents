package io.paritytech.polkadotapp.feature_products_api.model.derivation

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import uniffi.truapi.ProductId
import uniffi.truapi.productIdFromName

/**
 * RFC-0022 governance-reserved dotNS identities for built-in app features.
 *
 * Reserved names live in each network's own namespace (`dim2.dot` on Polkadot, `dim2.paseo`
 * on Paseo), so every id is a function of the network TLD — matching the runtime's
 * network-specific product providers (individuality#1300).
 */
object ReservedProductIds {
    fun game(tld: DotNsTld): ProductId = productIdFromName("dim2", tld.value)

    fun funding(tld: DotNsTld): ProductId = productIdFromName("fund", tld.value)

    fun lightPersonIdentity(tld: DotNsTld): ProductId = productIdFromName("uid", tld.value)

    fun personhood(tld: DotNsTld): ProductId = productIdFromName("peopl", tld.value)
}
