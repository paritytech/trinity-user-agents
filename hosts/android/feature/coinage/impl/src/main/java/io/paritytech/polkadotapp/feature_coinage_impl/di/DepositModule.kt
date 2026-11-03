package io.paritytech.polkadotapp.feature_coinage_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.deposit.AutoConvertDepositService
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.BalanceChangeTracker
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.FundsConverter
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.RealBalanceChangeTracker
import io.paritytech.polkadotapp.feature_coinage_impl.data.deposit.RealFundsConverter
import io.paritytech.polkadotapp.feature_coinage_impl.domain.deposit.RealAutoConvertDepositService

@Module
@InstallIn(SingletonComponent::class)
internal interface DepositModule {
    @Binds
    fun bindAutoConvertDepositService(real: RealAutoConvertDepositService): AutoConvertDepositService

    @Binds
    fun bindBalanceChangeTracker(real: RealBalanceChangeTracker): BalanceChangeTracker

    @Binds
    fun bindFundsConverter(real: RealFundsConverter): FundsConverter
}
