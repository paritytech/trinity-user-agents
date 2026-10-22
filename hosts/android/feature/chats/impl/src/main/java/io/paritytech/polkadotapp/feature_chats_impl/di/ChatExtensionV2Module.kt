package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatActions
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomRinger
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealPersonChatActions
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealProductFeedListeners

/** The bindings of Chat Extension v2 and its additions: room declarations and product rooms. */
@Module
@InstallIn(SingletonComponent::class)
interface ChatExtensionV2Module {
    @Binds
    fun bindChatRoomDeclarationsRepository(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsRepository

    @Binds
    fun bindChatRoomDeclarationsWriter(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsWriter

    @Binds
    fun bindProductFeedListeners(impl: RealProductFeedListeners): ProductFeedListeners

    @Binds
    fun bindChatRoomCallRepository(impl: RealChatRoomCallRepository): ChatRoomCallRepository

    @Binds
    fun bindChatRoomCallWriter(impl: RealChatRoomCallRepository): ChatRoomCallWriter

    @Binds
    fun bindChatRoomEventPublisher(impl: RealChatRoomEventPublisher): ChatRoomEventPublisher

    @Binds
    @IntoSet
    fun bindProductRoomRinger(impl: ProductRoomRinger): AppInitializer

    @Binds
    fun bindPersonChatActions(impl: RealPersonChatActions): PersonChatActions
}
