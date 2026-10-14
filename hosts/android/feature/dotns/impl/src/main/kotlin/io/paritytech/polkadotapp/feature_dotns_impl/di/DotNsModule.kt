package io.paritytech.polkadotapp.feature_dotns_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_impl.data.config.DotNsConfigProvider
import io.paritytech.polkadotapp.feature_dotns_impl.data.config.RemoteConfigDotNsConfigProvider
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.RealDotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.RealCarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.repository.NetworkSuffixRepository
import io.paritytech.polkadotapp.feature_dotns_impl.data.repository.RealNetworkSuffixRepository
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsTldStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.RealDotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.RealDotNsTldStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.SharedPrefsContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs.RealDotNsResolver
import io.paritytech.polkadotapp.feature_dotns_impl.domain.tld.RealDotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_impl.presentation.DotNsTldInitializer
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface DotNsModule {
    @Binds
    @Singleton
    fun bindDotNsResolver(impl: RealDotNsResolver): DotNsResolver

    @Binds
    @Singleton
    fun bindDotNsContractApi(impl: RealDotNsContractApi): DotNsContractApi

    @Binds
    @Singleton
    fun bindCarFetcher(impl: RealCarFetcher): CarFetcher

    @Binds
    @Singleton
    fun bindDotNsContentStorage(impl: RealDotNsContentStorage): DotNsContentStorage

    @Binds
    @Singleton
    fun bindContentHashCache(impl: SharedPrefsContentHashOverrides): ContentHashOverrides

    @Binds
    @Singleton
    fun bindDotNsConfigProvider(impl: RemoteConfigDotNsConfigProvider): DotNsConfigProvider

    @Binds
    @Singleton
    fun bindNetworkSuffixRepository(impl: RealNetworkSuffixRepository): NetworkSuffixRepository

    @Binds
    fun bindDotNsTldProvider(impl: RealDotNsTldProvider): DotNsTldProvider

    @Binds
    @Singleton
    fun bindDotNsTldStorage(impl: RealDotNsTldStorage): DotNsTldStorage

    @Binds
    @IntoSet
    fun bindDotNsTldInitializer(impl: DotNsTldInitializer): AppInitializer
}
