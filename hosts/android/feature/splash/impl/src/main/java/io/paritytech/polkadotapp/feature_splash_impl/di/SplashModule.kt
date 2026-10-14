package io.paritytech.polkadotapp.feature_splash_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_splash_api.presentation.SplashPassedObserver
import io.paritytech.polkadotapp.feature_splash_impl.presentation.RealSplashPassedObserver
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface SplashModule {
    @Binds
    @Singleton
    fun bindSplashPassedObserver(impl: RealSplashPassedObserver): SplashPassedObserver
}
