package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify

class RealApplyRemoteChatMessageUseCaseTest {
    private val peerAccountId: AccountId = ByteArray(32) { 7 }.toDataByteArray()

    private val chatEngine: ChatEngine = mock(ChatEngine::class.java)
    private val processedChatMessageRepository: ProcessedChatMessageRepository = mock(ProcessedChatMessageRepository::class.java)

    private val useCase = RealApplyRemoteChatMessageUseCase(
        chatMessageRepository = mock(ChatMessageRepository::class.java),
        contactsRepository = mock(ContactsRepository::class.java),
        chatEngine = chatEngine,
        accountRepository = mock(AccountRepository::class.java),
        chainRegistry = mock(ChainRegistry::class.java),
        knownChains = mock(KnownChains::class.java),
        processedChatMessageRepository = processedChatMessageRepository,
    )

    @Test
    fun `synced message is placed by its timestamp`() = runBlocking<Unit> {
        withNewMessageSaved()
        withMessageAlreadyProcessed()

        useCase.apply(encodedIncomingText(), peerAccountId, isOutgoing = false, status = ChatMessage.Status.NEW)

        verify(chatEngine).saveMessage(
            any(),
            eq(ChatMessageSaveConflictStrategy.IGNORE),
            eq(ChatMessagePlacement.ByTimestamp),
            any(),
        )
    }

    private suspend fun withNewMessageSaved() {
        whenever(chatEngine.saveMessage(any(), any(), any(), any())).thenReturn(true)
    }

    private suspend fun withMessageAlreadyProcessed() {
        whenever(processedChatMessageRepository.tryMarkProcessed(MESSAGE_ID)).thenReturn(false)
    }

    private fun encodedIncomingText(): ByteArray {
        return ChatMessage(
            id = MESSAGE_ID,
            chatId = ChatId.fromContact(peerAccountId),
            timestamp = 1_000L,
            origin = ChatMessageOrigin.Contact(peerAccountId),
            content = ChatMessage.Content.Text("hi"),
            status = ChatMessage.Status.NEW,
        ).toEncodedMessage().getOrThrow()
    }

    private companion object {
        const val MESSAGE_ID = "message-id"
    }
}
