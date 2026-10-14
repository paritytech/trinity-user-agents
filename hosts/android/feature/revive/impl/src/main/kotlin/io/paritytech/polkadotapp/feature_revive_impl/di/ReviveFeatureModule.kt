package io.paritytech.polkadotapp.feature_revive_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_impl.data.RealReviveContractApi
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface ReviveFeatureModule {
    @Binds
    @Singleton
    fun bindReviveContractApi(impl: RealReviveContractApi): ReviveContractApi
}
