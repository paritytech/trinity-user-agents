package io.paritytech.polkadotapp.chains.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.parity.truapi.LightClient
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ConnectionSecrets
import io.paritytech.polkadotapp.chains.multiNetwork.connection.provider.LightClientChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.provider.LightClientSettings
import io.paritytech.polkadotapp.chains.multiNetwork.connection.provider.LightClientStorage
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.types.TypesFetcher
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import javax.inject.Qualifier
import javax.inject.Singleton

@Qualifier
@Retention(AnnotationRetention.BINARY)
annotation class ForceProduction

@Module
@InstallIn(SingletonComponent::class)
class ChainRegistryModule {
    @Provides
    @Singleton
    fun provideKnownChains(environment: TestnetEnvironment) = KnownChains.createFor(environment)

    @Provides
    @Singleton
    @ForceProduction
    fun provideProductionKnownChains() = KnownChains.createFor(TestnetEnvironment.PRODUCTION)

    @Provides
    @Singleton
    fun provideTypesFetcher(networkApiCreator: NetworkApiCreator) =
        networkApiCreator.create(TypesFetcher::class.java)

    @Provides
    @Singleton
    fun provideConnectionSecrets(): ConnectionSecrets = ConnectionSecrets.default()

    @Provides
    @Singleton
    fun provideLightClient(
        storage: LightClientStorage,
        settings: LightClientSettings,
        environment: TestnetEnvironment,
    ) = LightClient(
        storage = storage,
        chains = if (settings.isEnabled()) LightClientChains.of(environment) else emptySet(),
    )
}
