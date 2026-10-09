package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.di

import androidx.lifecycle.SavedStateHandle
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingBottomSheet

@Module
@InstallIn(ViewModelComponent::class)
object FundingModule {
    @Provides
    fun provideFundingOverlayContext(
        savedStateHandle: SavedStateHandle,
        contexts: FundingOverlayContexts,
    ): FundingOverlayContext {
        val intent = requireNotNull(savedStateHandle.get<String>(FundingBottomSheet.INTENT)) { "Funding sheet opened without a session" }
        return requireNotNull(contexts.get(intent)) {
            "FundingOverlayContext is not set. The funding sheet was likely restored after process death."
        }
    }
}
