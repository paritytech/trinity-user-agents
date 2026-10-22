package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomContactModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RealDeleteRoomUseCaseTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")

    private val chatRoomRepository: ChatRoomRepository = mockk(relaxed = true)
    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val chatDraftRepository: ChatDraftRepository = mockk(relaxed = true)
    private val declarationsRepository: ChatRoomDeclarationsRepository = mockk(relaxed = true)
    private val declarationsWriter: ChatRoomDeclarationsWriter = mockk(relaxed = true)
    private val callWriter: ChatRoomCallWriter = mockk(relaxed = true)

    private val useCase = RealDeleteRoomUseCase(
        chatRoomRepository = chatRoomRepository,
        chatMessageRepository = chatMessageRepository,
        chatDraftRepository = chatDraftRepository,
        declarationsRepository = declarationsRepository,
        declarationsWriter = declarationsWriter,
        callWriter = callWriter,
    )

    @Test
    fun `deleting a person chat of the chat product removes its messages, draft, declarations and call`() = runBlocking<Unit> {
        val contact = ChatRoomContactModel(ByteArray(32) { 1 }.intoAccountId(), "bob", ChatContactStateModel.ESTABLISHED)
        coEvery { declarationsRepository.get(room) } returns ChatRoomDeclarations.EMPTY.copy(contact = contact)

        useCase.invoke(room)

        coVerify { chatRoomRepository.deleteRoom(room) }
        coVerify { chatMessageRepository.deleteAllChatMessages(room) }
        coVerify { chatDraftRepository.deleteDraft(room) }
        coVerify { declarationsWriter.delete(room) }
        verify { callWriter.set(room, null) }
    }

    @Test
    fun `deleting the room of another product keeps its draft, as before the chat product`() = runBlocking<Unit> {
        coEvery { declarationsRepository.get(room) } returns null

        useCase.invoke(room)

        coVerify { chatRoomRepository.deleteRoom(room) }
        coVerify { chatMessageRepository.deleteAllChatMessages(room) }
        coVerify(exactly = 0) { chatDraftRepository.deleteDraft(any()) }
    }
}
