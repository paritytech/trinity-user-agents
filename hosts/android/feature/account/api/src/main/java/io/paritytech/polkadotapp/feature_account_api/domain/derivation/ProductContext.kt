package io.paritytech.polkadotapp.feature_account_api.domain.derivation

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.intoBandersnatchContext
import io.paritytech.polkadotapp.common.utils.blake2b256

/**
 * RFC-0004 product context: `blake2b256(utf8("product/") ++ utf8(productName) ++ utf8("/") ++ suffix)`.
 *
 * Sits next to [DerivationIndex32] rather than in the products module so that products and the
 * personhood-owned features derive their contexts through the very same bytes.
 */
fun productContext(productName: String, suffix: DerivationIndex32): BandersnatchContext {
    val prefix = "product/$productName/".encodeToByteArray()

    return (prefix + suffix.bytes.value).blake2b256().intoBandersnatchContext()
}
