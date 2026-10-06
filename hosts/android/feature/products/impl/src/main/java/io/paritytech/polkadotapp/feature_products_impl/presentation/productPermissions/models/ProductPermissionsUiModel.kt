package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.AutomaticPreimagePermission

@Immutable
data class ProductPermissionsUiModel(
    val productName: String,
    val permissions: List<ProductPermissionStatus>,
    val automaticUploads: AutomaticPreimagePermission? = null,
    val automaticUploadsBusy: Boolean = false,
)
