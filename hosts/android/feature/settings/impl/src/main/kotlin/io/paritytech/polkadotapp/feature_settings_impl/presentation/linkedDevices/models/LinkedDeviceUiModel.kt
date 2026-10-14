package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models

import androidx.compose.runtime.Immutable

@Immutable
data class LinkedDeviceUiModel(
    val id: String,
    val name: String,
    val description: String?,
    val category: LinkedDeviceCategory
)
