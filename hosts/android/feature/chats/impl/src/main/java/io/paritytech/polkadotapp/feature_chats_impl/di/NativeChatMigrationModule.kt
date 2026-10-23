package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_api.domain.NativeChatPaymentAdoption
import io.paritytech.polkadotapp.feature_chats_impl.domain.export.RealNativeChatHistoryExporter
import io.paritytech.polkadotapp.feature_chats_impl.domain.export.RealNativeChatPaymentAdoption

/** The one-way move of an install from the native person chat to the chat product. */
@Module
@InstallIn(SingletonComponent::class)
interface NativeChatMigrationModule {
    @Binds
    fun bindNativeChatHistoryExporter(impl: RealNativeChatHistoryExporter): NativeChatHistoryExporter

    @Binds
    fun bindNativeChatPaymentAdoption(impl: RealNativeChatPaymentAdoption): NativeChatPaymentAdoption
}
