package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomDeclarationsRepository

/** The bindings of Chat Extension v2: room declarations and product rooms. */
@Module
@InstallIn(SingletonComponent::class)
interface ChatExtensionV2Module {
    @Binds
    fun bindChatRoomDeclarationsRepository(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsRepository

    @Binds
    fun bindChatRoomDeclarationsWriter(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsWriter
}
