package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import uniffi.truapi.ProductId
import javax.inject.Inject

class RealProductScriptResolver @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
) : ProductScriptResolver {
    override suspend fun resolveWorker(productId: ProductId): Result<ProductExecutable.Worker> {
        return resolveProductUseCase.resolve(productId).flatMap { resolved ->
            resolved.executables.worker?.let { Result.success(it) }
                ?: Result.failure(IllegalStateException("Product ${productId.value} exposes no worker executable"))
        }
    }
}
