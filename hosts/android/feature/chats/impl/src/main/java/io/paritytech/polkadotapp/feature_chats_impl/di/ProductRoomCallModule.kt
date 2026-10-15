package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallAlerts
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call.ProductRoomCallBannerRenderer

/** The calls of the product rooms outside their room screens (07-LATER §11.2). */
@Module
@InstallIn(SingletonComponent::class)
interface ProductRoomCallModule {
    @Binds
    @IntoSet
    fun bindProductRoomCallAlerts(impl: ProductRoomCallAlerts): AppInitializer

    companion object {
        // The banner decides itself when it shows, so no screen owns it.
        @Provides
        @IntoSet
        fun provideProductRoomCallOverlay(renderer: ProductRoomCallBannerRenderer): ChatOverlay =
            ChatOverlay(renderer = renderer, ownedFragmentClasses = emptySet())
    }
}
