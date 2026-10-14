package io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.models.DeviceDetailsUiState
import kotlinx.coroutines.flow.StateFlow

interface DeviceDetailsContract {
    val state: StateFlow<LoadingState<DeviceDetailsUiState>>

    fun onBackClick()

    fun onRemoveDeviceClick()

    fun onConfirmRemoveClick()

    fun onCancelRemoveClick()
}
