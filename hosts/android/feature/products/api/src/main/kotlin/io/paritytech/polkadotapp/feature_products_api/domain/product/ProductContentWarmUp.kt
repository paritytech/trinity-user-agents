package io.paritytech.polkadotapp.feature_products_api.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductId

fun interface ProductContentWarmUp {
    /**
     * Fetches the archive a product's Pocket card page is served from, ahead of hosting it. A caller that
     * knows which product is about to be opened can spend the wait it already has on the chain read
     * and the download, instead of in front of them.
     */
    suspend fun warmUp(productId: ProductId): Result<Unit>
}
