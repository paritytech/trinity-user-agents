package io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestContextHolder

@Module
@InstallIn(ViewModelComponent::class)
class PaymentRequestModule {
    @Provides
    fun providePaymentRequestContext(
        holder: PaymentRequestContextHolder,
    ): PaymentRequestContext {
        return requireNotNull(holder.get()) {
            "PaymentRequestContext is not set. The payment-request prompt was likely restored after process death."
        }
    }
}
