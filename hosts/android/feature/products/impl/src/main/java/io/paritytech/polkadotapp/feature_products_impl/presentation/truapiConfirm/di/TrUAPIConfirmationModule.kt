package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIConfirmationContext
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIConfirmationContextHolder

@Module
@InstallIn(ViewModelComponent::class)
class TrUAPIConfirmationModule {
    @Provides
    fun provideTrUAPIConfirmationContext(
        holder: TrUAPIConfirmationContextHolder,
    ): TrUAPIConfirmationContext {
        return requireNotNull(holder.get()) {
            "TrUAPIConfirmationContext is not set."
        }
    }
}
