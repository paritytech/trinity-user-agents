package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RealDeleteRoomUseCaseTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")

    private val chatRoomRepository: ChatRoomRepository = mockk(relaxed = true)
    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val declarationsWriter: ChatRoomDeclarationsWriter = mockk(relaxed = true)

    private val useCase = RealDeleteRoomUseCase(
        chatRoomRepository = chatRoomRepository,
        chatMessageRepository = chatMessageRepository,
        declarationsWriter = declarationsWriter,
    )

    // A room that its product creates again later starts without the declarations of the room before it.
    @Test
    fun `deleting a room removes its messages and its declarations`() = runBlocking<Unit> {
        useCase.invoke(room)

        coVerify { chatRoomRepository.deleteRoom(room) }
        coVerify { chatMessageRepository.deleteAllChatMessages(room) }
        coVerify { declarationsWriter.delete(room) }
    }
}
