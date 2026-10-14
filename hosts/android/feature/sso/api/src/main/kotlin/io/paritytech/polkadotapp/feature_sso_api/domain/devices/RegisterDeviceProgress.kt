package io.paritytech.polkadotapp.feature_sso_api.domain.devices

sealed interface RegisterDeviceProgress {
    data object Verifying : RegisterDeviceProgress
    data object Registering : RegisterDeviceProgress
    data object Done : RegisterDeviceProgress
    data class Failed(val error: Throwable) : RegisterDeviceProgress
}
