package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.feature_fund_impl.domain.fund.FundInteractor
import io.paritytech.polkadotapp.feature_fund_impl.domain.fund.RealFundInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface FundModule {
    @Binds
    @ViewModelScoped
    fun bindFundInteractor(interactor: RealFundInteractor): FundInteractor
}
