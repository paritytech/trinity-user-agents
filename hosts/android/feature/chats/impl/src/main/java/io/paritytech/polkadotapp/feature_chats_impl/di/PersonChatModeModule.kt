package io.paritytech.polkadotapp.feature_chats_impl.di

import android.content.Context
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_impl.BuildConfig
import io.paritytech.polkadotapp.feature_chats_impl.domain.export.RealNativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_impl.domain.mode.PrefsPersonChatMode
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface PersonChatModeModule {
    @Binds
    fun bindNativeChatHistoryExporter(impl: RealNativeChatHistoryExporter): NativeChatHistoryExporter

    companion object {
        @Provides
        @Singleton
        fun providePersonChatMode(@ApplicationContext context: Context): PersonChatMode =
            PrefsPersonChatMode(
                prefs = context.getSharedPreferences("person_chat_mode", Context.MODE_PRIVATE),
                isDebugBuild = BuildConfig.DEBUG,
            )
    }
}
