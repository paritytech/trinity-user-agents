package io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.models

import androidx.compose.runtime.Immutable

@Immutable
data class ProductSettingsUiModel(
    val name: String,
    val hasPermissions: Boolean,
    val iconUrl: String?,
)
