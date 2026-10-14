package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class LinkedDevicesSectionUiModel(
    val category: LinkedDeviceCategory,
    val devices: ImmutableList<LinkedDeviceUiModel>
)
