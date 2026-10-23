package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_chats_api.presentation.address.ContactsAddressConverterFactory
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.GeneralAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.ImageAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.TypedAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.VideoAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.RealPersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageProcessingRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MessageRevisionRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatMessageProcessingRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealMessageRevisionRepository
import io.paritytech.polkadotapp.feature_chats_impl.deeplink.RealChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatActiveTrackerInternal
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.RealChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_impl.domain.RealContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatRequestsListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealChatRequestsListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.CompoundExternalExtensionProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.RealDeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.RealGetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.username.RealFallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_impl.presentation.RealTextMessageDrawer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.address.RealContactsAddressConverterFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.RealChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization.NativeChatWorkCleanup
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface ChatsFeatureApiModule {
    @Binds
    @IntoSet
    fun bindNativeChatWorkCleanup(impl: NativeChatWorkCleanup): AppInitializer

    @Binds
    fun bindPersonChatNotifications(impl: RealPersonChatNotifications): PersonChatNotifications

    @Binds
    fun bindChatMessageRepository(impl: RealChatMessageRepository): ChatMessageRepository

    @Binds
    fun bindChatMessageTimeFormatter(impl: RealChatMessageTimeFormatter): ChatMessageTimeFormatter

    @Binds
    fun bindChatActiveTracker(impl: RealChatActiveTracker): ChatActiveTracker

    @Binds
    fun bindChatDraftRepository(impl: RealChatDraftRepository): ChatDraftRepository

    @Binds
    fun bindChatSearchRecentsRepository(impl: RealChatSearchRecentsRepository): ChatSearchRecentsRepository

    @Binds
    fun bindChatActiveTrackerInternal(impl: RealChatActiveTracker): ChatActiveTrackerInternal

    @Binds
    @Singleton
    fun bindChatMessageProcessingRepository(impl: RealChatMessageProcessingRepository): ChatMessageProcessingRepository

    @Binds
    fun bindMessageSender(chatEngine: ChatEngine): ChatMessageSender

    @Binds
    fun bindMessageRevisionRepository(impl: RealMessageRevisionRepository): MessageRevisionRepository

    @Binds
    fun bindFallbackUsernameGenerator(impl: RealFallbackUsernameGenerator): FallbackUsernameGenerator

    @Binds
    fun bindChatDeeplinkMapper(impl: RealChatDeeplinkMapper): ChatDeeplinkMapper

    @Binds
    fun bindExternalExtensionProvider(impl: CompoundExternalExtensionProvider): ExternalExtensionProvider

    @Binds
    @Singleton
    fun bindContactDisplayProvider(impl: RealContactDisplayProvider): ContactDisplayProvider

    @Binds
    @Singleton
    fun bindChatRoomRepository(impl: RealChatRoomRepository): ChatRoomRepository

    @Binds
    @Singleton
    fun bindReadOnlyChatRoomRepository(impl: RealChatRoomRepository): ReadOnlyChatRoomRepository

    @Binds
    fun bindChatRequestsListInteractor(impl: RealChatRequestsListInteractor): ChatRequestsListInteractor

    @Binds
    fun bindRealGetContactsUseCase(impl: RealGetContactsUseCase): GetContactsUseCase

    @Binds
    fun bindContactsAddressConverterFactory(impl: RealContactsAddressConverterFactory): ContactsAddressConverterFactory

    @Binds
    @IntoSet
    fun bindImageAttachmentMetaBuilder(impl: ImageAttachmentMetaBuilder): TypedAttachmentMetaBuilder

    @Binds
    @IntoSet
    fun bindVideoAttachmentMetaBuilder(impl: VideoAttachmentMetaBuilder): TypedAttachmentMetaBuilder

    @Binds
    @IntoSet
    fun bindGeneralAttachmentMetaBuilder(impl: GeneralAttachmentMetaBuilder): TypedAttachmentMetaBuilder

    @Binds
    fun bindDeleteRoomUseCase(impl: RealDeleteRoomUseCase): DeleteRoomUseCase

    @Binds
    fun bindTextMessageDrawer(impl: RealTextMessageDrawer): TextMessageDrawer
}
