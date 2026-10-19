package io.paritytech.polkadotapp.feature_products_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatExtensionProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock.MockChatRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.InAppProductRuntime

@Module
@InstallIn(SingletonComponent::class)
interface MockChatModule {
    @Binds
    @IntoSet
    fun bindMockChatRuntime(impl: MockChatRuntime): InAppProductRuntime

    @Binds
    @IntoSet
    fun bindMockChatExtensionProvider(impl: MockChatExtensionProvider): ExternalExtensionProvider
}
