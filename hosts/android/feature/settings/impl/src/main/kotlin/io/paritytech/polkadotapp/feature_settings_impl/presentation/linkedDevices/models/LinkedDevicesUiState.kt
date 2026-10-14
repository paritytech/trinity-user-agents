package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Immutable
data class LinkedDevicesUiState(
    val sections: ImmutableList<LinkedDevicesSectionUiModel> = persistentListOf()
) {
    val isEmpty: Boolean = sections.all { it.devices.isEmpty() }
}
