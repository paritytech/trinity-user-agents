package io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import javax.inject.Inject

interface SpaBrowserInteractor {
    suspend fun resolveProduct(productId: ProductId): Result<ResolvedProduct>
}

class RealSpaBrowserInteractor @Inject constructor(
    private val resolveProductUseCase: ResolveProductUseCase,
) : SpaBrowserInteractor {
    override suspend fun resolveProduct(productId: ProductId): Result<ResolvedProduct> =
        resolveProductUseCase.resolve(productId)
}
