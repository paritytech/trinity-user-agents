package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCache
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageCaching
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.ProductMessageSourceFactory
import io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource.RealProductMessageSourceFactory

/** The message sources of the room screen. */
@Module
@InstallIn(SingletonComponent::class)
interface ChatMessageSourceModule {
    @Binds
    fun bindProductMessageSourceFactory(impl: RealProductMessageSourceFactory): ProductMessageSourceFactory

    @Binds
    fun bindProductMessageCaching(impl: ProductMessageCache): ProductMessageCaching
}
