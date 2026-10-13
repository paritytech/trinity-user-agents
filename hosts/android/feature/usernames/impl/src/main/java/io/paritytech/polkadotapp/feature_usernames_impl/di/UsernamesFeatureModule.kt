package io.paritytech.polkadotapp.feature_usernames_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalFullUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.RecoverUsernameUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ResolveUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.SearchUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.ParseAddressUsernameConverterFactory
import io.paritytech.polkadotapp.feature_usernames_api.presentation.address.UsernameAddressConverterFactory
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.RealUsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.UsernameApi
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.fullUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.queuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.usernameStorage
import io.paritytech.polkadotapp.feature_usernames_impl.domain.RealUsernamesChainProvider
import io.paritytech.polkadotapp.feature_usernames_impl.domain.UsernamesChainProvider
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.RealObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.RealRecoverUsernameUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.RealResolveUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.RealSearchUsernamesUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.RealUsernamesOfAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.address.RealParseAddressUsernameConverterFactory
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.address.RealUsernameAddressConverterFactory
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.WidevineIntegrityInterceptor
import io.paritytech.polkadotapp.tools_jwt_auth_api.BearerAuth
import okhttp3.ConnectionPool
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import java.util.concurrent.TimeUnit
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface UsernamesFeatureApiModule {
    companion object {
        @Provides
        fun provideUsernameStorage(factory: SingleValueStorageFactory): LocalUsernameStorage =
            factory.usernameStorage()

        @Provides
        fun provideFullUsernameStorage(factory: SingleValueStorageFactory): LocalFullUsernameStorage =
            factory.fullUsernameStorage()

        @Provides
        fun provideQueuedClaimStorage(factory: SingleValueStorageFactory): QueuedClaimStorage =
            factory.queuedClaimStorage()

        @Provides
        fun provideUsernamesApi(
            networkApiCreator: NetworkApiCreator,
            @BearerAuth bearerOkHttpClient: OkHttpClient,
            widevineIntegrityInterceptor: WidevineIntegrityInterceptor,
        ): UsernameApi {
            val clientBuilder = bearerOkHttpClient.newBuilder()
            clientBuilder.interceptors().add(0, widevineIntegrityInterceptor)

            return networkApiCreator
                .createRetrofit(customOkHttpClient = clientBuilder.build())
                .create(UsernameApi::class.java)
        }

        @Provides
        fun provideRetrofit(
            networkApiCreator: NetworkApiCreator,
            builder: OkHttpClient.Builder,
        ): Retrofit {
            // We don't need many connections for Usernames - reduce consumed resources by this instance of OkhttpClient
            val reducedConnectionPool = ConnectionPool(
                maxIdleConnections = 1,
                keepAliveDuration = 5,
                timeUnit = TimeUnit.SECONDS
            )
            builder.connectionPool(reducedConnectionPool)
            val retrofit = networkApiCreator.createRetrofit(
                customOkHttpClient = builder.build()
            )
            return retrofit
        }
    }

    @Binds
    fun bindUsernameUseCase(impl: RealUsernamesOfAccountUseCase): UsernameOfAccountUseCase

    @Binds
    fun bindRecoverUsernameUseCase(impl: RealRecoverUsernameUseCase): RecoverUsernameUseCase

    @Binds
    fun bindResolveUsernamesUseCase(impl: RealResolveUsernamesUseCase): ResolveUsernamesUseCase

    @Binds
    fun bindUsernameClaimRepository(impl: RealUsernameRepository): UsernameRepository

    @Binds
    @Singleton
    fun bindUsernameAddressConverterFactory(implementation: RealUsernameAddressConverterFactory): UsernameAddressConverterFactory

    @Binds
    @Singleton
    fun bindParserAddressUsernameConverterFactory(implementation: RealParseAddressUsernameConverterFactory): ParseAddressUsernameConverterFactory

    @Binds
    fun bindUsernamesChainProvider(implementation: RealUsernamesChainProvider): UsernamesChainProvider

    @Binds
    fun bindSearchUsernamesUseCase(implementation: RealSearchUsernamesUseCase): SearchUsernamesUseCase

    @Binds
    fun bindObserveAccountOnboardingStatusUseCase(implementation: RealObserveAccountOnboardingStatusUseCase): ObserveAccountOnboardingStatusUseCase
}
