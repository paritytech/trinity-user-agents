package io.paritytech.polkadotapp.feature_chats_impl.di

import android.content.Context
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileStore
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.RealChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatMediaLoader
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.RealChatMediaLoader
import java.io.File
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface ChatRoomPolicyModule {
    @Binds
    fun bindChatRoomPolicyProvider(impl: RealChatRoomPolicyProvider): ChatRoomPolicyProvider

    @Binds
    fun bindChatMediaLoader(impl: RealChatMediaLoader): ChatMediaLoader

    companion object {
        @Provides
        @Singleton
        fun provideChatResourceDiskCache(@ApplicationContext context: Context): ChatResourceDiskCache =
            ChatResourceDiskCache(File(context.cacheDir, "chat_resources"), ChatResourceDiskCache.MAX_BYTES)

        @Provides
        @Singleton
        fun provideChatHopFileStore(@ApplicationContext context: Context): ChatHopFileStore =
            ChatHopFileStore(ChatResourceDiskCache(File(context.filesDir, "chat_sealed_files"), ChatHopFileStore.MAX_BYTES))
    }
}
