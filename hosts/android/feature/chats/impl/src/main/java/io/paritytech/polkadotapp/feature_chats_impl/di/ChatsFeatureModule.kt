package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatOriginCustomConfiguration
import io.paritytech.polkadotapp.feature_chats_api.presentation.ChatStarter
import io.paritytech.polkadotapp.feature_chats_impl.RealChatStarter
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.ChatSearchInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatSearch.RealChatSearchInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.AddContactInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealAddContactInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealChatListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.RealWaitForChatExistsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.WaitForChatExistsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.scan.ContactAddressScanContentParser
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser

@Module
@InstallIn(SingletonComponent::class)
interface ChatsFeatureModule {
    @Binds
    fun bindChatListInteractor(impl: RealChatListInteractor): ChatListInteractor

    @Binds
    fun bindChatSearchInteractor(impl: RealChatSearchInteractor): ChatSearchInteractor

    @Binds
    fun bindAddContactInteractor(impl: RealAddContactInteractor): AddContactInteractor

    @Binds
    fun bindWaitForChatExistsUseCase(impl: RealWaitForChatExistsUseCase): WaitForChatExistsUseCase

    @Binds
    @IntoSet
    fun bindContactAddressScanContentParser(impl: ContactAddressScanContentParser): ScanContentParser

    @Binds
    fun bindChatStarter(real: RealChatStarter): ChatStarter

    @Multibinds
    fun customChatHeaderRenderers(): Map<String, CustomChatHeaderRenderer>

    @Multibinds
    fun chatOriginCustomConfigurations(): Map<String, ChatOriginCustomConfiguration>
}
