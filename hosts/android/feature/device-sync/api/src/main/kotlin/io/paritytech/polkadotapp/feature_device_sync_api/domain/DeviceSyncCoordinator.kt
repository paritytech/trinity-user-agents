package io.paritytech.polkadotapp.feature_device_sync_api.domain

/** Inter-own-device sync entry point. Started once at app launch; process-lifetime. */
interface DeviceSyncCoordinator {
    fun startSubscriptions()
}
