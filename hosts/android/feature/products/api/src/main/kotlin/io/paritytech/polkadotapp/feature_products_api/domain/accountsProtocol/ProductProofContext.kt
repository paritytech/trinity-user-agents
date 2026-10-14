package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.productContext
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

data class ProductProofContext(
    val productId: ProductId,
    val suffix: DerivationIndex32,
) {
    fun productContextBytes(): BandersnatchContext = productContext(productId.value, suffix)
}
