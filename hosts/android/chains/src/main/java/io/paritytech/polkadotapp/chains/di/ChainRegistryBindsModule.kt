package io.paritytech.polkadotapp.chains.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.ChainFetcher
import io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.RemoteConfigChainFetcher
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.RealChainConnectionRefCounter
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface ChainRegistryBindsModule {
    @Binds
    @Singleton
    fun bindHardCodedChainFetcher(real: RemoteConfigChainFetcher): ChainFetcher

    @Binds
    fun bindChainConnectionRefCounter(impl: RealChainConnectionRefCounter): ChainConnectionRefCounter
}
