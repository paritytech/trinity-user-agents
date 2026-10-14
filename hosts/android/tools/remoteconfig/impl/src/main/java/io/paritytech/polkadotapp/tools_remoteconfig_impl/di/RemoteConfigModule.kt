package io.paritytech.polkadotapp.tools_remoteconfig_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_impl.data.RemoteConfigDataSource
import io.paritytech.polkadotapp.tools_remoteconfig_impl.data.sources.FirebaseRemoteConfigDataSource
import io.paritytech.polkadotapp.tools_remoteconfig_impl.domain.RealRemoteConfigService
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface RemoteConfigModule {
    @Binds
    @Singleton
    fun bindRemoteConfigDataSource(impl: FirebaseRemoteConfigDataSource): RemoteConfigDataSource

    @Binds
    @Singleton
    fun bindRemoteConfigService(impl: RealRemoteConfigService): RemoteConfigService
}
