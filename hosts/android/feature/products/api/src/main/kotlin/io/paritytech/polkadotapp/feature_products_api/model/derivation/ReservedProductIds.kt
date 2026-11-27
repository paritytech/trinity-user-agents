package io.paritytech.polkadotapp.feature_products_api.model.derivation

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import uniffi.truapi.PrivilegedProduct
import uniffi.truapi.ProductId
import uniffi.truapi.privilegedProductId

/**
 * RFC-0022 governance-reserved dotNS identities for built-in app features.
 *
 * Reserved names live in each network's own namespace (`dim2.dot` on Polkadot, `dim2.paseo`
 * on Paseo), so every id is a function of the network TLD — matching the runtime's
 * network-specific product providers (individuality#1300).
 *
 * [personhood] account 0 is the wallet's main account (RFC-0028).
 */
object ReservedProductIds {
    fun game(tld: DotNsTld): ProductId = privilegedProductId(PrivilegedProduct.GAME, tld.value)

    fun funding(tld: DotNsTld): ProductId = ProductId.fromName("fund", tld.value).getOrThrow()

    fun personhood(tld: DotNsTld): ProductId = privilegedProductId(PrivilegedProduct.PERSONHOOD, tld.value)

    fun chat(tld: DotNsTld): ProductId = ProductId.fromName("chat", tld.value).getOrThrow()

    fun all(tld: DotNsTld): Set<ProductId> = setOf(game(tld), funding(tld), personhood(tld), chat(tld))
}
