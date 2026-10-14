package io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.models

import androidx.compose.runtime.Immutable

@Immutable
data class DeviceDetailsUiState(
    val deviceLabel: String?,
    val hostLabel: String?,
    val addedLabel: String?,
    val isRemoving: Boolean = false,
    val isRemoveConfirmationVisible: Boolean = false,
)
