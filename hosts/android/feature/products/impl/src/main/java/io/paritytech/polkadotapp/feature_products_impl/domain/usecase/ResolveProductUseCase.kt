package io.paritytech.polkadotapp.feature_products_impl.domain.usecase

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct

/**
 * Resolves a product from its dotNS identity. Manifest and debug-menu products share one shape, so
 * callers just branch on [ResolvedProduct.executables].
 *
 * An absent manifest record falls back to a legacy APP-only stub; a read failure or a malformed
 * manifest surfaces as `Result.failure`. Resolutions are cached, so repeat calls are cheap.
 */
interface ResolveProductUseCase {
    suspend fun resolve(productId: ProductId): Result<ResolvedProduct>

    /** Drops the cached entry for [productId], so the next resolve fetches afresh. */
    suspend fun invalidate(productId: ProductId)
}
