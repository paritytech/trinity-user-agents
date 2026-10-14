package io.paritytech.polkadotapp.feature_transactions_impl.di

import com.google.gson.Gson
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.SignerProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.DefaultTransactionExtensionProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.TxPayloadExtensionsResolver
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.FreeTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.LitePeopleOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.TestnetTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ResubmitWhenValidFactory
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.di.ExtrinsicSerializer
import io.paritytech.polkadotapp.feature_transactions_impl.data.DefaultExtrinsicVersionProvider
import io.paritytech.polkadotapp.feature_transactions_impl.data.ExtrinsicBuilderFactory
import io.paritytech.polkadotapp.feature_transactions_impl.data.RealDefaultExtrinsicVersionProvider
import io.paritytech.polkadotapp.feature_transactions_impl.data.RealExtrinsicBuilderFactory
import io.paritytech.polkadotapp.feature_transactions_impl.data.RealExtrinsicService
import io.paritytech.polkadotapp.feature_transactions_impl.data.extensions.RealTxPayloadExtensionsResolver
import io.paritytech.polkadotapp.feature_transactions_impl.data.extrinsicSerializer.ExtrinsicSerializers
import io.paritytech.polkadotapp.feature_transactions_impl.data.origins.RealFreeTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions_impl.data.origins.RealLitePeopleOrigins
import io.paritytech.polkadotapp.feature_transactions_impl.data.origins.RealSignedOrigins
import io.paritytech.polkadotapp.feature_transactions_impl.data.origins.RealTestnetTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions_impl.data.retry.RealResubmitWhenValidFactory
import io.paritytech.polkadotapp.feature_transactions_impl.data.signer.RealSignerProvider
import io.paritytech.polkadotapp.feature_transactions_impl.data.tracked.RealTrackedExtrinsicService
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.ExtrinsicValidator
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.RealExtrinsicValidator
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface TransactionsFeatureApiModule {
    @Binds
    fun bindExtrinsicBuilderFactory(impl: RealExtrinsicBuilderFactory): ExtrinsicBuilderFactory

    @Binds
    fun bindExtrinsicService(impl: RealExtrinsicService): ExtrinsicService

    @Binds
    fun bindExtrinsicValidator(impl: RealExtrinsicValidator): ExtrinsicValidator

    @Binds
    fun bindResubmitWhenValidFactory(impl: RealResubmitWhenValidFactory): ResubmitWhenValidFactory

    @Binds
    @Singleton
    fun bindTrackedExtrinsicService(impl: RealTrackedExtrinsicService): TrackedExtrinsicService

    @Binds
    fun bindSignerProvider(impl: RealSignerProvider): SignerProvider

    @Binds
    fun bindDefaultExtrinsicVersionProvider(impl: RealDefaultExtrinsicVersionProvider): DefaultExtrinsicVersionProvider

    @Binds
    fun bindTxPayloadExtensionsResolver(impl: RealTxPayloadExtensionsResolver): TxPayloadExtensionsResolver

    @Binds
    @Singleton
    fun bindFreeTxOrigins(impl: RealFreeTransactionOrigins): FreeTransactionOrigins

    @Binds
    @Singleton
    fun bindSignedOrigins(impl: RealSignedOrigins): SignedOrigins

    @Binds
    fun bindTestnetOrigins(impl: RealTestnetTransactionOrigins): TestnetTransactionOrigins

    @Binds
    @Singleton
    fun bindLitePeopleOrigins(impl: RealLitePeopleOrigins): LitePeopleOrigins

    @Multibinds
    fun defaultTransactionExtensionProviders(): Set<DefaultTransactionExtensionProvider>

    companion object TransactionsFeatureProvidesModule {
        @Provides
        @Singleton
        @ExtrinsicSerializer
        fun provideExtrinsicSerializerGson(): Gson = ExtrinsicSerializers.gson()
    }
}
