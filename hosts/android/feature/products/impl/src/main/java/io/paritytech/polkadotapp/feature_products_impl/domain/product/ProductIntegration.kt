package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ProductId

class ProductIntegration(
    val productId: ProductId,
    val type: IntegrationType,
)

sealed class IntegrationType {
    data object Chat : IntegrationType()
}
