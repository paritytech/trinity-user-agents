package io.paritytech.polkadotapp.feature_products_api.domain

import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

interface ProductAccountIdProvider {
    suspend fun deriveAccountId(productAccountId: ProductAccountId): Result<EncodedPublicKey>

    /**
     * Public key of `//product//{productId}`. RFC-0022: a Host that has this key can derive every
     * account public key in the product's subtree locally, without a round trip per account.
     */
    suspend fun deriveProductSubtreePublicKey(productId: ProductId): Result<EncodedPublicKey>
}
