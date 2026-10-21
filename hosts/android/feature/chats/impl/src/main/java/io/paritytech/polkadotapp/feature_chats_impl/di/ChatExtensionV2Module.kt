package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRegistry
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatBotPersonaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomUserStateWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.BitmapProductImageShrinker
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.ProductImageShrinker
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealChatRoomEventPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealProductFeedListeners
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.RealProductRoomAdditionsFeed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud.ChatReadAloudController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.readAloud.TextToSpeechReadAloudController

/** The bindings of Chat Extension v2 and its additions: room declarations and product rooms. */
@Module
@InstallIn(SingletonComponent::class)
interface ChatExtensionV2Module {
    @Binds
    fun bindChatRoomDeclarationsRepository(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsRepository

    @Binds
    fun bindChatRoomDeclarationsWriter(impl: RealChatRoomDeclarationsRepository): ChatRoomDeclarationsWriter

    @Binds
    fun bindProductImageShrinker(impl: BitmapProductImageShrinker): ProductImageShrinker

    @Binds
    fun bindProductFeedListeners(impl: RealProductFeedListeners): ProductFeedListeners

    @Binds
    fun bindChatRoomUserStateRepository(impl: RealChatRoomUserStateRepository): ChatRoomUserStateRepository

    @Binds
    fun bindChatRoomUserStateWriter(impl: RealChatRoomUserStateRepository): ChatRoomUserStateWriter

    @Binds
    fun bindChatRoomCallRepository(impl: RealChatRoomCallRepository): ChatRoomCallRepository

    @Binds
    fun bindChatRoomCallWriter(impl: RealChatRoomCallRepository): ChatRoomCallWriter

    @Binds
    fun bindChatBotPersonaRepository(impl: RealChatBotPersonaRepository): ChatBotPersonaRepository

    @Binds
    fun bindChatBotPersonaRegistry(impl: RealChatBotPersonaRepository): ChatBotPersonaRegistry

    @Binds
    fun bindChatBotPersonaWriter(impl: RealChatBotPersonaRepository): ChatBotPersonaWriter

    @Binds
    fun bindChatRoomEventPublisher(impl: RealChatRoomEventPublisher): ChatRoomEventPublisher

    @Binds
    fun bindProductRoomAdditionsFeed(impl: RealProductRoomAdditionsFeed): ProductRoomAdditionsFeed

    @Binds
    fun bindChatReadAloudController(impl: TextToSpeechReadAloudController): ChatReadAloudController
}
