package io.paritytech.polkadotapp.tools_push_notifications_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import io.paritytech.polkadotapp.tools_push_notifications_impl.RealVanillaPushNotificationHelper
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface PushNotificationsVanillaFeatureModule {
    @Binds
    @Singleton
    fun bindPushNotificationsHelper(impl: RealVanillaPushNotificationHelper): PushNotificationsHelper

}
