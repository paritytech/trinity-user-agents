package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_impl.domain.browser.ProductSources
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import uniffi.truapi.ProductId
import javax.inject.Inject

class RealProductScriptResolver @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val productSources: ProductSources,
) : ProductScriptResolver {
    override suspend fun resolveWorker(productId: ProductId): Result<ResolvedWorker> {
        return resolveProductUseCase.resolve(productId).mapCatching { resolved ->
            val worker = resolved.executables.worker
                ?: error("Product ${productId.value} exposes no worker executable")
            ResolvedWorker(worker, productSources.of(productId.value))
        }
    }
}
