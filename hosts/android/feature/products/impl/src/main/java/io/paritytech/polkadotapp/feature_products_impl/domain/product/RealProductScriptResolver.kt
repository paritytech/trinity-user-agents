package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_impl.domain.browser.ProductSources
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource
import javax.inject.Inject

/** Reads a Worker from the bundle the app ships when the product is bundled, otherwise from its published manifest. */
class RealProductScriptResolver @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
    private val productSources: ProductSources,
    private val bundledProducts: BundledProducts,
) : ProductScriptResolver {
    override suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker> {
        val source = productSources.of(productId.value)
        if (source is ProductSource.Bundled) return bundledProducts.worker(source.entry)
        return resolveProductUseCase.resolve(productId).mapCatching { resolved ->
            resolved.executables.worker
                ?: error("Product ${productId.value} exposes no worker executable")
        }
    }
}
