package io.paritytech.polkadotapp.feature_xcm_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_xcm_api.builder.XcmBuilder
import io.paritytech.polkadotapp.feature_xcm_api.config.XcmConfigRepository
import io.paritytech.polkadotapp.feature_xcm_api.converter.LocationConverterFactory
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.xcmPayment.XcmPaymentApi
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.feature_xcm_impl.builder.RealXcmBuilderFactory
import io.paritytech.polkadotapp.feature_xcm_impl.config.RealXcmConfigRepository
import io.paritytech.polkadotapp.feature_xcm_impl.converter.asset.RealLocationConverterFactory
import io.paritytech.polkadotapp.feature_xcm_impl.runtimeApi.dryRun.RealDryRunApi
import io.paritytech.polkadotapp.feature_xcm_impl.runtimeApi.xcmPayment.RealXcmPaymentApi
import io.paritytech.polkadotapp.feature_xcm_impl.versions.detector.RealXcmVersionDetector
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface XcmFeatureApiModule {
    @Binds
    @Singleton
    fun bindXcmVersionDetector(real: RealXcmVersionDetector): XcmVersionDetector

    @Binds
    @Singleton
    fun bindDryRunApi(real: RealDryRunApi): DryRunApi

    @Binds
    @Singleton
    fun bindXcmPaymentApi(real: RealXcmPaymentApi): XcmPaymentApi

    @Binds
    @Singleton
    fun bindXcmBuilderFactory(real: RealXcmBuilderFactory): XcmBuilder.Factory

    @Binds
    @Singleton
    fun bindXcmConfigRepository(real: RealXcmConfigRepository): XcmConfigRepository

    @Binds
    @Singleton
    fun bindLocationConverterFactory(real: RealLocationConverterFactory): LocationConverterFactory
}
