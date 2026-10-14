package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32

/**
 * RFC-0022: past the host API boundary an account within a product is always identified by its
 * 32-byte index — the plain/raw distinction only exists on the wire.
 */
data class ProductAccountId(val productId: String, val index: DerivationIndex32)

val ProductAccountId.productIdTyped: ProductId
    get() = ProductId.fromStoredValue(productId)
