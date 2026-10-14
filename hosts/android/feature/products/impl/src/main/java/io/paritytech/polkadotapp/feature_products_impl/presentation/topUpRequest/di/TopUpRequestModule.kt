package io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpRequestContextHolder

@Module
@InstallIn(ViewModelComponent::class)
object TopUpRequestModule {
    @Provides
    fun provideTopUpRequestContext(
        holder: TopUpRequestContextHolder,
    ): TopUpRequestContext {
        return requireNotNull(holder.get()) {
            "TopUpRequestContext is not set. The top-up prompt was likely restored after process death."
        }
    }
}
