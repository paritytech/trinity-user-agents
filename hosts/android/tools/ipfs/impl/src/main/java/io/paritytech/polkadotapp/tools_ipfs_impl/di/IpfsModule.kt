package io.paritytech.polkadotapp.tools_ipfs_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_impl.data.RealIpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_impl.data.config.IpfsGatewayUrlProvider
import io.paritytech.polkadotapp.tools_ipfs_impl.data.config.RemoteConfigIpfsGatewayUrlProvider
import io.paritytech.polkadotapp.tools_ipfs_impl.data.image.IpfsImageLoaderRegistrar

@Module
@InstallIn(SingletonComponent::class)
internal interface IpfsModule {
    @Binds
    fun bindIpfsContentLookup(impl: RealIpfsContentLookup): IpfsContentLookup

    @Binds
    fun bindIpfsGatewayUrlProvider(impl: RemoteConfigIpfsGatewayUrlProvider): IpfsGatewayUrlProvider

    @Binds
    @IntoSet
    fun bindIpfsImageLoaderRegistrar(impl: IpfsImageLoaderRegistrar): ImageLoaderComponentRegistrar
}
