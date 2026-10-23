package io.paritytech.polkadotapp.feature_chats_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_calls_api.domain.ExternalCallSignaling
import io.paritytech.polkadotapp.feature_calls_api.domain.IncomingCallGate
import io.paritytech.polkadotapp.feature_chats_api.deeplink.ChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_api.domain.BlockedContactsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBroadcastUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactChatSessionManager
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDirectory
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.chatRequest.ChatRequestServiceCoordinator
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.BroadcastDeviceLifecycleUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.OurDevicesProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.AddContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.ApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.RemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.notifications.IncomingChatPushDecoder
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.DeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_chats_api.presentation.address.ContactsAddressConverterFactory
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.MultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.GeneralAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.ImageAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.TypedAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.attachment.VideoAttachmentMetaBuilder
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestCrypto
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestProver
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.RealChatRequestCrypto
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.RealChatRequestProver
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload.CompressImages
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload.FileUploadPreProcessor
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatMessageNotificationSentRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatPushNotificationHandler
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.RealChatMessageNotificationSentRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.RealIncomingChatPushDecoder
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.RealPersonChatNotifications
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageProcessingRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.CompactionExpansionRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.MessageRevisionRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatMessageProcessingRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatRoomRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealCompactionExpansionRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealContactDevicesRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealContactDirectory
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealMessageRevisionRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RealRemovedChatsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.RemovedChatsRepository
import io.paritytech.polkadotapp.feature_chats_impl.deeplink.RealChatDeeplinkMapper
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatActiveTrackerInternal
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.RealChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_impl.domain.RealChatBroadcastUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.RealContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.calls.RealExternalCallSignaling
import io.paritytech.polkadotapp.feature_chats_impl.domain.calls.RealIncomingCallGate
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.ChatRequestAcceptProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.ChatRequestDiscoveryService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.IncomingChatRequestVerifier
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.OutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.RealChatRequestDiscoveryService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.RealChatRequestServiceCoordinator
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.RealIncomingChatRequestProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.RealIncomingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.RealIncomingChatRequestVerifier
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.RealOutgoingChatRequestService
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTransport
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.RealChatRequestTransport
import io.paritytech.polkadotapp.feature_chats_impl.domain.deviceLifecycle.DeviceLifecycleMessageProcessor
import io.paritytech.polkadotapp.feature_chats_impl.domain.devices.RealBroadcastDeviceLifecycleUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.devices.RealContactDeviceProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.devices.RealOurDevicesProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.ChatRequestsListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealAddContactUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealChatRequestsListInteractor
import io.paritytech.polkadotapp.feature_chats_impl.domain.interactors.RealRemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.CompoundExternalExtensionProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.notifications.ChatPushNotificationsSender
import io.paritytech.polkadotapp.feature_chats_impl.domain.notifications.RealChatPushNotificationsSender
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.RealContactChatSessionManager
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.RealContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.RealDeleteRoomUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.RealGetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.username.RealFallbackUsernameGenerator
import io.paritytech.polkadotapp.feature_chats_impl.presentation.RealTextMessageDrawer
import io.paritytech.polkadotapp.feature_chats_impl.presentation.address.RealContactsAddressConverterFactory
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.RealChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization.NativePersonChatServices
import io.paritytech.polkadotapp.feature_chats_impl.presentation.transfer.MultimediaMessageStateHolder
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.ContactDeviceProvider
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationHandler
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface ChatsFeatureApiModule {
    @Binds
    fun bindContactsRepository(impl: RealContactsRepository): ContactsRepository

    @Binds
    fun bindContactDevicesRepository(impl: RealContactDevicesRepository): ContactDevicesRepository

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
    fun bindChatRequestRepository(impl: RealChatRequestRepository): ChatRequestRepository

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
