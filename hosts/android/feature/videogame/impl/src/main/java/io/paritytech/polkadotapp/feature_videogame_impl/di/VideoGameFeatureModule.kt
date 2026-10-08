package io.paritytech.polkadotapp.feature_videogame_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserFragmentClass
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.calendar.RealProductGameCalendar
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.RealVideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameCalendar
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameNotificationAutoCanceller
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameOsAccess
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealProductGameReminder
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealVideoGameReminderScheduler
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.VideoGameReminderScheduler
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.autoLaunch.ProductGameAutoOpener
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.GamePillOverlayRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.notifications.RealProductGameOsAccess

@InstallIn(SingletonComponent::class)
@Module
internal interface VideoGameFeatureModule {
    @Binds
    fun bindVideoGameReminderScheduler(impl: RealVideoGameReminderScheduler): VideoGameReminderScheduler

    @Binds
    fun bindProductGameReminder(impl: RealProductGameReminder): ProductGameReminder

    @Binds
    fun bindProductGameCalendar(impl: RealProductGameCalendar): ProductGameCalendar

    @Binds
    fun bindProductGameOsAccess(impl: RealProductGameOsAccess): ProductGameOsAccess

    @Binds
    fun bindVideoGameNotificationPublisher(impl: RealVideoGameNotificationPublisher): VideoGameNotificationPublisher

    @Binds
    @IntoSet
    fun bindProductGameAutoOpener(impl: ProductGameAutoOpener): AppInitializer

    @Binds
    @IntoSet
    fun bindProductGameNotificationAutoCanceller(impl: ProductGameNotificationAutoCanceller): AppInitializer

    companion object {
        // Owned fragments hide an overlay: the pill stays off while a product is on screen.
        @Provides
        @IntoSet
        fun provideProductGamePill(@SpaBrowserFragmentClass spaBrowserFragmentClass: String): ChatOverlay = ChatOverlay(
            renderer = GamePillOverlayRenderer(),
            ownedFragmentClasses = setOf(spaBrowserFragmentClass),
        )
    }
}
