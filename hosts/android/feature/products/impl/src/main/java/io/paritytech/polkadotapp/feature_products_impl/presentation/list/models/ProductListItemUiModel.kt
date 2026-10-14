package io.paritytech.polkadotapp.feature_products_impl.presentation.list.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

@Immutable
data class ProductListItemUiModel(
    val id: ProductId,
    val name: String,
    val iconUrl: String?,
)
