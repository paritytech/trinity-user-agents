package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.SavedProductPermission

@Immutable
data class ProductPermissionsUiModel(
    val productName: String,
    val permissions: List<SavedProductPermission>
)
