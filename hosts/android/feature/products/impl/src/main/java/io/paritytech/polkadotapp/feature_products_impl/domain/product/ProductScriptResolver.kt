package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource

/** Resolves a product's background worker executable, carrying its full script URL. */
interface ProductScriptResolver {
    suspend fun resolveWorker(productId: ProductId): Result<ResolvedWorker>
}

/** A product's worker and where it runs from, decided together so its WebView serves the script from there. */
data class ResolvedWorker(
    val executable: ProductExecutable.Worker,
    val source: ProductSource?,
)
