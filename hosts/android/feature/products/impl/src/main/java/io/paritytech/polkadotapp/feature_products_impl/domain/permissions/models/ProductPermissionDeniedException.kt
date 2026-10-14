package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models

class ProductPermissionDeniedException(
    val permission: ProductPermission,
) : Exception("Permission denied: ${permission.typeName}/${permission.key}")
