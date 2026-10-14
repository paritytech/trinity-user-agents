package io.paritytech.polkadotapp.feature_fund_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_fund_api.domain.AutoConvertDepositService
import io.paritytech.polkadotapp.feature_fund_impl.data.BalanceChangeTracker
import io.paritytech.polkadotapp.feature_fund_impl.data.FundsConverter
import io.paritytech.polkadotapp.feature_fund_impl.data.RealBalanceChangeTracker
import io.paritytech.polkadotapp.feature_fund_impl.data.RealFundsConverter
import io.paritytech.polkadotapp.feature_fund_impl.domain.RealAutoConvertDepositService

@Module
@InstallIn(SingletonComponent::class)
internal interface FundFeatureApiModule {
    @Binds
    fun bindAutoConvertDepositService(real: RealAutoConvertDepositService): AutoConvertDepositService

    @Binds
    fun bindBalanceChangeTracker(real: RealBalanceChangeTracker): BalanceChangeTracker

    @Binds
    fun bindFundsConverter(real: RealFundsConverter): FundsConverter
}
