package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateService
import io.paritytech.polkadotapp.feature_chats_impl.domain.userState.RealChatRoomUserStateService

/** The binding of the mute and the archive that the user sets for a product room (07-LATER §6). */
@Module
@InstallIn(SingletonComponent::class)
interface ChatRoomUserStateModule {
    @Binds
    fun bindChatRoomUserStateService(impl: RealChatRoomUserStateService): ChatRoomUserStateService
}
