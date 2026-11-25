package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPublisher
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatActiveTrackerInternal
import org.junit.Test

class ChatFeedInteractorTest {
    private val chatNotificationPublisher: ChatNotificationPublisher = mockk(relaxed = true)
    private val chatActiveTracker: ChatActiveTrackerInternal = mockk(relaxed = true)

    private val interactor = ChatFeedInteractor(
        chatMessageRepository = mockk(),
        chatDraftRepository = mockk(),
        chatSessionManager = mockk(),
        chatNotificationPublisher = chatNotificationPublisher,
        chatActiveTracker = chatActiveTracker,
        chatEngine = mockk(),
        roomPolicyProvider = mockk(),
        contactsRepository = mockk(),
        coroutineDispatchers = mockk(),
        chatDisplayGenerator = mockk(),
        dmChatMessageOriginDisplayResolver = mockk(),
        accountRepository = mockk(),
        addContactUseCase = mockk(),
        deleteLocalChat = mockk(),
        incomingChatRequestProcessor = mockk(),
        callController = mockk(),
        callStateTracker = mockk(),
        fileUploadRepository = mockk(),
        fileUploadStarter = mockk(),
        attachmentMetaBuilder = mockk(),
        attachmentFileStorage = mockk(),
        hopNodeUrlProvider = mockk(),
        chatSigners = mockk(),
        identitySigningConsent = mockk(),
        roomSourceContextFactory = mockk(),
        attachmentStager = mockk(),
        chatSeenRepository = mockk(),
        chatRequestRepository = mockk(),
        outgoingChatRequestCanceller = mockk(),
    )

    private val chatId = ChatId.fromRawValue(byteArrayOf(0x01))

    @Test
    fun `setChatActive marks the chat active and cancels its notification`() {
        interactor.setChatActive(chatId)

        verify { chatActiveTracker.setActive(chatId) }
        verify { chatNotificationPublisher.cancelChatNotification(chatId) }
    }
}
