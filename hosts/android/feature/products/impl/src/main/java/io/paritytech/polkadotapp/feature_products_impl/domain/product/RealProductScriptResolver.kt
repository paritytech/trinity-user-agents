package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import javax.inject.Inject

class RealProductScriptResolver @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
) : ProductScriptResolver {
    override suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker> {
        return resolveProductUseCase.resolve(productId).mapCatching { resolved ->
            resolved.executables.worker
                ?: error("Product ${productId.value} exposes no worker executable")
        }
    }
}
