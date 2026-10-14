package io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContextHolder

@Module
@InstallIn(ViewModelComponent::class)
class ResourceAllocationRequestModule {
    @Provides
    fun provideResourceAllocationRequestContext(
        holder: ResourceAllocationRequestContextHolder,
    ): ResourceAllocationRequestContext {
        return requireNotNull(holder.get()) {
            "ResourceAllocationRequestContext is not set."
        }
    }
}
