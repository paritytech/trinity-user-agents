package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_impl.data.model.toEncodedMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ProcessedChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveConflictStrategy
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RealApplyRemoteChatMessageUseCaseTest {
    private val peerAccountId: AccountId = ByteArray(32) { 7 }.toDataByteArray()

    private val chatEngine: ChatEngine = mockk()
    private val processedChatMessageRepository: ProcessedChatMessageRepository = mockk()

    private val useCase = RealApplyRemoteChatMessageUseCase(
        chatMessageRepository = mockk(),
        contactsRepository = mockk(),
        chatEngine = chatEngine,
        accountRepository = mockk(),
        chainRegistry = mockk(),
        knownChains = mockk(),
        processedChatMessageRepository = processedChatMessageRepository,
    )

    @Test
    fun `synced message is placed by its timestamp`() = runBlocking<Unit> {
        withNewMessageSaved()
        withMessageAlreadyProcessed()

        useCase.apply(encodedIncomingText(), peerAccountId, isOutgoing = false, status = ChatMessage.Status.NEW)

        coVerify(exactly = 1) {
            chatEngine.saveMessage(
                any(),
                ChatMessageSaveConflictStrategy.IGNORE,
                ChatMessagePlacement.ByTimestamp,
                any(),
            )
        }
    }

    private suspend fun withNewMessageSaved() {
        coEvery { chatEngine.saveMessage(any(), any(), any(), any()) } returns true
    }

    private suspend fun withMessageAlreadyProcessed() {
        coEvery { processedChatMessageRepository.tryMarkProcessed(MESSAGE_ID) } returns false
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
