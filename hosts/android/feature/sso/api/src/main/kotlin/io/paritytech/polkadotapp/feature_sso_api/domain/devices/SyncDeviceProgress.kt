package io.paritytech.polkadotapp.feature_sso_api.domain.devices

sealed interface SyncDeviceProgress {
    data object Syncing : SyncDeviceProgress
    data object Done : SyncDeviceProgress
    data class Failed(val error: Throwable) : SyncDeviceProgress
}
