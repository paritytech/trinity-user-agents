package io.paritytech.polkadotapp.feature_w3spay_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoMap
import dagger.multibindings.IntoSet
import dagger.multibindings.StringKey
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.feature_coinage_api.domain.submitter.CoinsSubmitter
import io.paritytech.polkadotapp.feature_scan_api.domain.DeeplinkScanContentParser
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import io.paritytech.polkadotapp.feature_w3spay_impl.data.config.RealW3sMerchantConfigRepository
import io.paritytech.polkadotapp.feature_w3spay_impl.data.config.W3sMerchantConfigRepository
import io.paritytech.polkadotapp.feature_w3spay_impl.deeplink.W3sPayDeepLinkHandler
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3S_COINS_SUBMITTER_ID
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3sCoinsSubmitter
import io.paritytech.polkadotapp.feature_w3spay_impl.presentation.scan.DsFinVkScanContentParser
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface W3sPayConfigModule {
    @Binds
    @Singleton
    fun bindW3sMerchantConfigRepository(impl: RealW3sMerchantConfigRepository): W3sMerchantConfigRepository
}

@Module
@InstallIn(SingletonComponent::class)
internal interface W3sPayModule {
    @Binds
    @IntoSet
    fun bindW3sPayDeepLinkHandler(impl: W3sPayDeepLinkHandler): DeepLinkHandler

    @Binds
    @IntoSet
    fun bindDsFinVkScanContentParser(impl: DsFinVkScanContentParser): ScanContentParser

    @Binds
    @IntoMap
    @StringKey(W3S_COINS_SUBMITTER_ID)
    fun bindW3sCoinsSubmitter(impl: W3sCoinsSubmitter): CoinsSubmitter

    companion object {
        @Provides
        @IntoSet
        fun provideW3sPayScanContentParser(handler: W3sPayDeepLinkHandler): ScanContentParser =
            DeeplinkScanContentParser(handler)
    }
}
