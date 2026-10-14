package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain.RealSendPaymentInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.domain.SendPaymentInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface SendPaymentModule {
    @Binds
    @ViewModelScoped
    fun bindSendPaymentInteractor(impl: RealSendPaymentInteractor): SendPaymentInteractor
}
