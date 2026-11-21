package io.paritytech.polkadotapp.feature_wallet_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_wallet_impl.domain.funding.CoinageFundingBalance
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding.FundingActivityMixin
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding.RealFundingActivityMixin

@Module
@InstallIn(SingletonComponent::class)
internal interface WalletFeatureModule {
    @Binds
    fun bindFundingBalance(impl: CoinageFundingBalance): FundingBalance

    @Binds
    fun bindFundingActivityMixinFactory(impl: RealFundingActivityMixin.Factory): FundingActivityMixin.Factory
}
