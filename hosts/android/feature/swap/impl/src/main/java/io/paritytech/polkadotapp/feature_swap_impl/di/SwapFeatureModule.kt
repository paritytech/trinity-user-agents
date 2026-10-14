package io.paritytech.polkadotapp.feature_swap_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_swap_api.domain.SwapService
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.PathQuoter
import io.paritytech.polkadotapp.feature_swap_impl.data.paths.RealPathQuoterFactory
import io.paritytech.polkadotapp.feature_swap_impl.domain.AssetInAdditionalSwapDeductionUseCase
import io.paritytech.polkadotapp.feature_swap_impl.domain.RealAssetInAdditionalSwapDeductionUseCase
import io.paritytech.polkadotapp.feature_swap_impl.domain.RealSwapService
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface SwapFeatureApiModule {
    @Binds
    fun bindAssetInAdditionalSwapDeductionUseCase(real: RealAssetInAdditionalSwapDeductionUseCase): AssetInAdditionalSwapDeductionUseCase

    @Binds
    @Singleton
    fun bindQuoterFactory(real: RealPathQuoterFactory): PathQuoter.Factory

    @Binds
    @Singleton
    fun bindSwapService(real: RealSwapService): SwapService
}
