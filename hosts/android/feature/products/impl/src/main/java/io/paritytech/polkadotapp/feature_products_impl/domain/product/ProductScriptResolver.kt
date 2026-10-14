package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/** Resolves a product's background worker executable, carrying its full script URL. */
interface ProductScriptResolver {
    suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker>
}
