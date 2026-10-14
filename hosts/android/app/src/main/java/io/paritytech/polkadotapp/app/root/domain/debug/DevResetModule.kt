package io.paritytech.polkadotapp.app.root.domain.debug

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_splash_api.domain.DevResetCoordinator
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface DevResetModule {
    @Binds
    @Singleton
    fun bindDevResetCoordinator(impl: RealDevResetCoordinator): DevResetCoordinator
}
