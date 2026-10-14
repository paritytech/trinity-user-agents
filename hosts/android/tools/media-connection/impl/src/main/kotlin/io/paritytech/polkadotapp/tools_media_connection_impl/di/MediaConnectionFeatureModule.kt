package io.paritytech.polkadotapp.tools_media_connection_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.tools_jwt_auth_api.BearerAuth
import io.paritytech.polkadotapp.tools_media_connection_api.domain.PeerChannelFactory
import io.paritytech.polkadotapp.tools_media_connection_impl.RealPeerChannelFactory
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.TurnApi
import okhttp3.OkHttpClient
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface MediaConnectionFeatureModule {
    @Binds
    fun bindPeerChannelFactory(impl: RealPeerChannelFactory): PeerChannelFactory

    companion object {
        @Provides
        @Singleton
        fun provideTurnApi(
            networkApiCreator: NetworkApiCreator,
            @BearerAuth bearerOkHttpClient: OkHttpClient,
        ): TurnApi = networkApiCreator
            .createRetrofit(customOkHttpClient = bearerOkHttpClient)
            .create(TurnApi::class.java)
    }
}
