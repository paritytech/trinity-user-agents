package io.paritytech.polkadotapp.tools_assethub_sdk_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_assethub_sdk_api.AssetHubSdk
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.AssetHubSwapSdkFactory

@Module
@InstallIn(SingletonComponent::class)
internal interface AssetHubSdkFeatureModule {
    @Binds
    fun bindAssetHubSwapSdkFactory(real: AssetHubSwapSdkFactory): AssetHubSdk.Factory
}
