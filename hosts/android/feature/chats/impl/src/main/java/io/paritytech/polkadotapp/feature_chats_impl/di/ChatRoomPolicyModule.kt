package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.RealChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.RealChatResourceResolver

@Module
@InstallIn(SingletonComponent::class)
interface ChatRoomPolicyModule {
    @Binds
    fun bindChatRoomPolicyProvider(impl: RealChatRoomPolicyProvider): ChatRoomPolicyProvider

    @Binds
    fun bindChatResourceResolver(impl: RealChatResourceResolver): ChatResourceResolver
}
