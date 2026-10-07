package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotStateController
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_chats_impl.domain.extension.CoinagePaymentProcessingExtension
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.bot.RealChatBotStateController
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.bot.sample.SampleBot
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface ChatBotsModule {
    @Binds
    @Singleton
    @IntoSet
    fun bindCoinagePaymentProcessingExtension(impl: CoinagePaymentProcessingExtension): ChatExtension

    @Binds
    @Singleton
    @IntoSet
    fun bindSampleBot(impl: SampleBot): ChatExtension

    @Binds
    @Singleton
    fun bindChatBotStateController(impl: RealChatBotStateController): ChatBotStateController

    @Multibinds
    fun chatOverlays(): Set<ChatOverlay>
}
