package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatBotPersonaWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatDraftRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RealDeleteRoomUseCaseTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")

    private val chatRoomRepository: ChatRoomRepository = mockk(relaxed = true)
    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val chatDraftRepository: ChatDraftRepository = mockk(relaxed = true)
    private val declarationsWriter: ChatRoomDeclarationsWriter = mockk(relaxed = true)
    private val callWriter: ChatRoomCallWriter = mockk(relaxed = true)
    private val personaWriter: ChatBotPersonaWriter = mockk(relaxed = true)

    private val useCase = RealDeleteRoomUseCase(
        chatRoomRepository = chatRoomRepository,
        chatMessageRepository = chatMessageRepository,
        chatDraftRepository = chatDraftRepository,
        declarationsWriter = declarationsWriter,
        callWriter = callWriter,
        personaWriter = personaWriter,
    )

    @Test
    fun `deleting a room removes its messages, draft, declarations and call`() = runBlocking<Unit> {
        useCase.invoke(room)

        coVerify { chatRoomRepository.deleteRoom(room) }
        coVerify { chatMessageRepository.deleteAllChatMessages(room) }
        coVerify { chatDraftRepository.deleteDraft(room) }
        coVerify { declarationsWriter.delete(room) }
        verify { callWriter.set(room, null) }
    }

    @Test
    fun `uninstalling a product removes each of its rooms and its bot personas`() = runBlocking<Unit> {
        val other = ChatId.forExtensionRoom("ProductBot_game.dot", "scores")
        every { chatRoomRepository.subscribeRoomsByExtension("ProductBot_game.dot") } returns flowOf(listOf(room, other))

        useCase.deleteAllRoomsOf("ProductBot_game.dot")

        coVerify { chatRoomRepository.deleteRoom(room) }
        coVerify { chatRoomRepository.deleteRoom(other) }
        coVerify { declarationsWriter.delete(other) }
        coVerify { personaWriter.deleteAll("ProductBot_game.dot") }
    }
}
