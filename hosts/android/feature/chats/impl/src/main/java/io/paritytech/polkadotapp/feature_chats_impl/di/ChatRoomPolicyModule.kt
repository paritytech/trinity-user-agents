package io.paritytech.polkadotapp.feature_chats_impl.di

import android.content.Context
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatSealedFileStore
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.RealChatRoomPolicyProvider
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatMediaLoader
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ProductMediaController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.RealChatMediaLoader
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.RealChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.RealProductMediaController
import java.io.File
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface ChatRoomPolicyModule {
    @Binds
    fun bindChatRoomPolicyProvider(impl: RealChatRoomPolicyProvider): ChatRoomPolicyProvider

    @Binds
    fun bindChatResourceResolver(impl: RealChatResourceResolver): ChatResourceResolver

    @Binds
    fun bindChatMediaLoader(impl: RealChatMediaLoader): ChatMediaLoader

    @Binds
    fun bindProductMediaController(impl: RealProductMediaController): ProductMediaController

    companion object {
        @Provides
        @Singleton
        fun provideChatResourceDiskCache(@ApplicationContext context: Context): ChatResourceDiskCache =
            ChatResourceDiskCache(File(context.cacheDir, "chat_resources"), ChatResourceDiskCache.MAX_BYTES)

        @Provides
        @Singleton
        fun provideChatSealedFileStore(@ApplicationContext context: Context): ChatSealedFileStore =
            ChatSealedFileStore(ChatResourceDiskCache(File(context.filesDir, "chat_sealed_files"), ChatSealedFileStore.MAX_BYTES))
    }
}
