package io.paritytech.polkadotapp.tools_hydration_sdk_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapSdk
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.RealHydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector.HydrationFeeInjector
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.injector.RealHydrationFeeInjector
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSdkFactory
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.HydrationSwapSource
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.aave.AaveHydrationSwapSourceFactory
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.OmniPoolHydrationSwapSourceFactory
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.StableSwapHydrationSwapSourceFactory
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.XYKHydrationSwapSourceFactory
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface HydrationModule {
    @Binds
    @Singleton
    fun bindAssetIdConverter(real: RealHydraDxAssetIdConverter): HydraDxAssetIdConverter

    @Binds
    @Singleton
    fun bindFeeInjector(real: RealHydrationFeeInjector): HydrationFeeInjector

    @Binds
    @IntoSet
    fun bindOmnipoolFactory(real: OmniPoolHydrationSwapSourceFactory): HydrationSwapSource.Factory

    @Binds
    @IntoSet
    fun bindAaveFactory(real: AaveHydrationSwapSourceFactory): HydrationSwapSource.Factory

    @Binds
    @IntoSet
    fun bindXykFactory(real: XYKHydrationSwapSourceFactory): HydrationSwapSource.Factory

    @Binds
    @IntoSet
    fun bindStableswapFactory(real: StableSwapHydrationSwapSourceFactory): HydrationSwapSource.Factory

    @Binds
    @Singleton
    fun bindSdkFactory(real: HydrationSwapSdkFactory): HydrationSwapSdk.Factory
}
