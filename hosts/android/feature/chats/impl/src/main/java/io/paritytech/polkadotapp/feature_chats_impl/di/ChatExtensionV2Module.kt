package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomActivityRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomActivityRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.BitmapProductImageShrinker
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.ProductImageShrinker
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealProductFeedListeners

/** The bindings of Chat Extension v2: room declarations, the activity line and product rooms. */
@Module
@InstallIn(SingletonComponent::class)
interface ChatExtensionV2Module {
    @Binds
    fun bindChatRoomDeclarationsRepository(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsRepository

    @Binds
    fun bindChatRoomDeclarationsWriter(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsWriter

    @Binds
    fun bindChatRoomActivityRepository(impl: RealChatRoomActivityRepository): ChatRoomActivityRepository

    @Binds
    fun bindChatRoomActivityWriter(impl: RealChatRoomActivityRepository): ChatRoomActivityWriter

    @Binds
    fun bindProductImageShrinker(impl: BitmapProductImageShrinker): ProductImageShrinker

    @Binds
    fun bindProductFeedListeners(impl: RealProductFeedListeners): ProductFeedListeners
}
