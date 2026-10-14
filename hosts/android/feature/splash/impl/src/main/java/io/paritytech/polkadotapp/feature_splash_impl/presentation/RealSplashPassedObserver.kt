package io.paritytech.polkadotapp.feature_splash_impl.presentation

import io.paritytech.polkadotapp.feature_splash_api.presentation.SplashPassedObserver
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealSplashPassedObserver @Inject constructor() : SplashPassedObserver {
    private val splashPassed = MutableStateFlow(false)

    override suspend fun awaitSplashPassed() {
        splashPassed.first { it }
    }

    override fun setSplashPassed() {
        splashPassed.value = true
    }
}
