package io.paritytech.polkadotapp.tools_push_notifications_impl.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.tools_jwt_auth_api.BearerAuth
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.LocalPushTokenStorage
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.api.NotifyApi
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.network.api.PushSubscriptionApi
import io.paritytech.polkadotapp.tools_push_notifications_impl.data.pushTokenStorage
import okhttp3.OkHttpClient

@Module
@InstallIn(SingletonComponent::class)
interface PushNotificationsFeatureModule {
    companion object {
        @Provides
        fun provideTokenStorage(factory: SingleValueStorageFactory): LocalPushTokenStorage =
            factory.pushTokenStorage()

        @Provides
        fun provideNotifyApi(
            networkApiCreator: NetworkApiCreator,
            @BearerAuth bearerOkHttpClient: OkHttpClient,
        ): NotifyApi = networkApiCreator
            .createRetrofit(customOkHttpClient = bearerOkHttpClient)
            .create(NotifyApi::class.java)

        @Provides
        fun providePushSubscriptionApi(
            networkApiCreator: NetworkApiCreator,
            @BearerAuth bearerOkHttpClient: OkHttpClient,
        ): PushSubscriptionApi = networkApiCreator
            .createRetrofit(customOkHttpClient = bearerOkHttpClient)
            .create(PushSubscriptionApi::class.java)
    }
}
