package io.paritytech.polkadotapp.feature_products_impl.presentation.crossProductProof.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContext
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContextHolder

@Module
@InstallIn(ViewModelComponent::class)
class CrossProductProofModule {
    @Provides
    fun provideCrossProductProofContext(
        holder: CrossProductProofContextHolder,
    ): CrossProductProofContext {
        return requireNotNull(holder.get()) {
            "CrossProductProofContext is not set."
        }
    }
}
