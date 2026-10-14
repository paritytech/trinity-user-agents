package io.paritytech.polkadotapp.feature_splash_api.presentation

interface SplashPassedObserver {
    suspend fun awaitSplashPassed()
    fun setSplashPassed()
}
