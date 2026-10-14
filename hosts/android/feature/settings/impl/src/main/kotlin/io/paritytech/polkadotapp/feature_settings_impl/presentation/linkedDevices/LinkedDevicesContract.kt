package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices

import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDevicesUiState
import kotlinx.coroutines.flow.StateFlow

interface LinkedDevicesContract {
    val state: StateFlow<LinkedDevicesUiState>

    fun onBackClick()

    fun onAddDeviceClick()

    fun onHowItWorksClick()

    fun onDeviceClick(deviceId: String)
}
