package io.paritytech.polkadotapp.feature_splash_api.domain

interface DevResetCoordinator {
    suspend fun clearAllAndClose()
}
