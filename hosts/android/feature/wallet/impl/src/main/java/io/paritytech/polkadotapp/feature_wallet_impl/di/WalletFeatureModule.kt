package io.paritytech.polkadotapp.feature_wallet_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_wallet_impl.data.config.AppSharingConfigRepository
import io.paritytech.polkadotapp.feature_wallet_impl.data.config.RealAppSharingConfigRepository

@Module
@InstallIn(SingletonComponent::class)
internal interface WalletFeatureModule {
    @Binds
    fun bindAppSharingConfigRepository(impl: RealAppSharingConfigRepository): AppSharingConfigRepository
}
