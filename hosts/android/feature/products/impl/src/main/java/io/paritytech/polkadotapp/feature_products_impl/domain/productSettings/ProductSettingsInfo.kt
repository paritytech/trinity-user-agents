package io.paritytech.polkadotapp.feature_products_impl.domain.productSettings

import io.paritytech.polkadotapp.feature_products_api.model.Product

data class ProductSettingsInfo(
    val product: Product,
    val hasPermissions: Boolean,
    val iconUrl: String?,
)
