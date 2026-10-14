package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.common.utils.Identifiable

// Must stay a data class — the product extension provider diffs on it.
data class Product(
    val id: ProductId,
    val name: String,
    val icon: ProductIcon?,
) : Identifiable {
    override val identifier: String get() = id.value
}
