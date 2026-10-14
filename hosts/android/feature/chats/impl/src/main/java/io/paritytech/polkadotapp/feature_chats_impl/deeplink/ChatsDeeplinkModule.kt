package io.paritytech.polkadotapp.feature_chats_impl.deeplink

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.feature_scan_api.domain.DeeplinkScanContentParser
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser

@Module
@InstallIn(SingletonComponent::class)
internal interface ChatsDeeplinkModule {
    @Binds
    @IntoSet
    fun bindChatDeepLinkHandler(impl: ChatDeepLinkHandler): DeepLinkHandler

    @Binds
    @IntoSet
    fun bindChatListDeepLinkHandler(impl: ChatListDeepLinkHandler): DeepLinkHandler

    companion object {
        @Provides
        @IntoSet
        fun provideChatScanContentParser(handler: ChatDeepLinkHandler): ScanContentParser =
            DeeplinkScanContentParser(handler)

        @Provides
        @IntoSet
        fun provideChatListScanContentParser(handler: ChatListDeepLinkHandler): ScanContentParser =
            DeeplinkScanContentParser(handler)
    }
}
