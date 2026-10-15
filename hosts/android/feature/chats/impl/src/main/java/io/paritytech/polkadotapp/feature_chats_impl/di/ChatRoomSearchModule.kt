package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomSearchCandidates
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatRoomSearchCandidates
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.ChatRoomSearchInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RealChatRoomSearchInteractor

/** The bindings of the search inside one open room (07-LATER §8). */
@Module
@InstallIn(SingletonComponent::class)
interface ChatRoomSearchModule {
    @Binds
    fun bindChatRoomSearchCandidates(impl: RealChatRoomSearchCandidates): ChatRoomSearchCandidates

    @Binds
    fun bindChatRoomSearchInteractor(impl: RealChatRoomSearchInteractor): ChatRoomSearchInteractor
}
