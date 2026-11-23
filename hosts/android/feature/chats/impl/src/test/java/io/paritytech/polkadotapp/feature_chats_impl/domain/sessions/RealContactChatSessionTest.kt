package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.anyChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.notifications.ChatNotificationPayloadEncoder
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.seen.SeenMessageProvider
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SyncContactUsernameUseCase
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionEvent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.capture
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.mockito.ArgumentCaptor
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import kotlin.reflect.KClass

class RealContactChatSessionTest {
    private val contactAccountId = ByteArray(32) { 1 }.intoAccountId()
    private val chatId = ChatId.fromContact(contactAccountId)

    private val chatMessageRepository = mock(ChatMessageRepository::class.java)
    private val chatEngine = mock(ChatEngine::class.java)
    private val syncContactUsernameUseCase = mock(SyncContactUsernameUseCase::class.java)
    private val communicationSession = mock(CommunicationSession::class.java)
    private val sessionEvents = MutableSharedFlow<CommunicationSessionEvent>(replay = 1)

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

    @Before
    fun setUp() = runBlocking<Unit> {
        whenever(communicationSession.remoteAccount).thenReturn(SessionAccount.Local(contactAccountId, null))
        whenever(communicationSession.incomingSessionId).thenReturn(ByteArray(32))
        whenever(communicationSession.generateSharedIncomingSessionValue(any())).thenReturn(ByteArray(32))
        whenever(communicationSession.generateSharedOutgoingSessionValue(any())).thenReturn(ByteArray(32))
        whenever(communicationSession.subscribeEvents()).thenReturn(sessionEvents)
        whenever(syncContactUsernameUseCase.sync(contactAccountId)).thenReturn(emptyFlow())
        whenever(chatEngine.subscribeOutgoingMessagesByStatus(chatId, ChatMessage.Status.NEW)).thenReturn(flowOf(emptyList()))
        whenever(chatEngine.subscribeOutgoingMessagesByStatus(chatId, ChatMessage.Status.IS_SENT)).thenReturn(flowOf(emptyList()))
        whenever(chatMessageRepository.getUnsupportedMessages(chatId)).thenReturn(emptyList())
    }

    @After
    fun tearDown() {
        scope.cancel()
    }

    @Test
    fun `keeps only chat requests and acceptances sent when the session finds none of its statements`() = runBlocking<Unit> {
        startSession()

        sessionEvents.emit(CommunicationSessionEvent.SentMessagesNotFound)

        @Suppress("UNCHECKED_CAST")
        val excluded = ArgumentCaptor.forClass(List::class.java) as ArgumentCaptor<List<KClass<out ChatMessage.Content>>>
        verify(chatMessageRepository).updateOutgoingMessagesStatusForChatExcludingTypes(
            anyChatId(),
            eq(ChatMessage.Status.IS_SENT),
            eq(ChatMessage.Status.NEW),
            capture(excluded)
        )
        assertEquals(
            setOf(ChatMessage.Content.ChatRequest::class, ChatMessage.Content.DeviceChatAccepted::class),
            excluded.value.toSet()
        )
    }

    private fun startSession() {
        val contact = mock(Contact::class.java)
        whenever(contact.accountId).thenReturn(contactAccountId)

        RealContactChatSession(
            scope = scope,
            contact = contact,
            communicationSessions = CommunicationSessions.Pairwise(communicationSession),
            chatMessageRepository = chatMessageRepository,
            callbacks = mock(ChatSessionCallbacks::class.java),
            chatEngine = chatEngine,
            syncContactUsernameUseCase = syncContactUsernameUseCase,
            incomingChatMessageProcessor = mock(IncomingChatMessageProcessor::class.java),
            notificationPayloadEncoder = mock(ChatNotificationPayloadEncoder::class.java),
            seenMessageProvider = mock(SeenMessageProvider::class.java),
            transientMessageProcessor = mock(TransientMessageProcessor::class.java),
        )
    }
}
