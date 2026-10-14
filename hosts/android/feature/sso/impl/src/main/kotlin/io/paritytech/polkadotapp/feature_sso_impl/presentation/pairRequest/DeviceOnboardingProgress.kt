package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest

sealed interface DeviceOnboardingProgress {
    data object Verifying : DeviceOnboardingProgress
    data object Registering : DeviceOnboardingProgress
    data object Syncing : DeviceOnboardingProgress
    data object Done : DeviceOnboardingProgress
    data class Failed(val error: Throwable) : DeviceOnboardingProgress
}
