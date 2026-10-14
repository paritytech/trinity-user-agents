package io.paritytech.polkadotapp.feature_chain_resources_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_impl.data.repository.RealResourcesRepository

@Module
@InstallIn(SingletonComponent::class)
interface ChainResourcesFeatureApiModule {
    @Binds
    fun bindResourcesRepository(impl: RealResourcesRepository): ResourcesRepository
}
