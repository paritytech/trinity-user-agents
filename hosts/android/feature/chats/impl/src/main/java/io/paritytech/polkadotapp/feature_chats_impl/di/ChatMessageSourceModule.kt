package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCaching

/** The message sources of the room screen. */
@Module
@InstallIn(SingletonComponent::class)
interface ChatMessageSourceModule {
    @Binds
    fun bindProductMessageCaching(impl: ProductMessageCache): ProductMessageCaching
}
