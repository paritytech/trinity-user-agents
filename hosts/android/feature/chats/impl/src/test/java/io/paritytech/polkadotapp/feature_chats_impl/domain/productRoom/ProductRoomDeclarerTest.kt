package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomActivityModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomConfigModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFooterModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomMetadataModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomActivityWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductRoomDeclarerTest {
    private val extensionId = "ProductBot_game.dot"
    private val room = ChatId.forExtensionRoom(extensionId, "lobby")

    private val chatRoomRepository: ChatRoomRepository = mockk(relaxed = true)
    private val chatMessageRepository: ChatMessageRepository = mockk(relaxed = true)
    private val declarationsRepository: ChatRoomDeclarationsRepository = mockk()
    private val declarationsWriter: ChatRoomDeclarationsWriter = mockk(relaxed = true)
    private val activityWriter: ChatRoomActivityWriter = mockk(relaxed = true)

    private val declarer = ProductRoomDeclarer(
        chatRoomRepository = chatRoomRepository,
        chatMessageRepository = chatMessageRepository,
        declarationsRepository = declarationsRepository,
        declarationsWriter = declarationsWriter,
        activityWriter = activityWriter,
    )

    @Test
    fun `a declaration for a room that does not exist fails as an unknown room`() = runBlocking<Unit> {
        withRoomExisting(false)

        val result = declarer.setFooter(extensionId, room, ChatRoomFooterModel.Empty)

        assertUnknownRoom(result)
        coVerify(exactly = 0) { declarationsWriter.setFooter(any(), any()) }
    }

    @Test
    fun `a declaration for the room of another extension fails as an unknown room`() = runBlocking<Unit> {
        withRoomExisting(true)

        val result = declarer.setFooter("ProductBot_other.dot", room, ChatRoomFooterModel.Empty)

        assertUnknownRoom(result)
    }

    @Test
    fun `the same configuration starts a new generation and keeps the messages`() = runBlocking<Unit> {
        withRoomExisting(true)
        withDeclaredConfig(null)

        declarer.setConfig(extensionId, room, ChatRoomConfigModel.HOST_DEFAULT)

        coVerify(exactly = 1) { declarationsWriter.setConfig(room, ChatRoomConfigModel.HOST_DEFAULT) }
        coVerify(exactly = 0) { chatMessageRepository.deleteAllChatMessages(any()) }
    }

    @Test
    fun `a change of the store mode discards the stored messages`() = runBlocking<Unit> {
        withRoomExisting(true)
        withDeclaredConfig(null)
        val productSource = ChatRoomConfigModel.HOST_DEFAULT.copy(productSource = true)

        declarer.setConfig(extensionId, room, productSource)

        coVerify { chatMessageRepository.deleteAllChatMessages(room) }
        coVerify { declarationsWriter.setConfig(room, productSource) }
    }

    @Test
    fun `a change that keeps the store mode keeps the messages`() = runBlocking<Unit> {
        withRoomExisting(true)
        withDeclaredConfig(ChatRoomConfigModel.HOST_DEFAULT)
        val seen = ChatRoomConfigModel.HOST_DEFAULT.copy(reportSeen = true)

        declarer.setConfig(extensionId, room, seen)

        coVerify(exactly = 0) { chatMessageRepository.deleteAllChatMessages(any()) }
        coVerify { declarationsWriter.setConfig(room, seen) }
    }

    @Test
    fun `metadata renames the room and keeps the description and avatar as declarations`() = runBlocking<Unit> {
        withRoomExisting(true)
        val metadata = ChatRoomMetadataModel(description = "Play", avatar = null)

        declarer.setMetadata(extensionId, room, "Lobby", metadata)

        coVerify { chatRoomRepository.renameRoom(room, "Lobby") }
        coVerify { declarationsWriter.setMetadata(room, metadata) }
    }

    @Test
    fun `the activity line goes to the in-memory store`() = runBlocking<Unit> {
        withRoomExisting(true)
        val activity = ChatRoomActivityModel("typing", participantId = null)

        declarer.setActivity(extensionId, room, activity)

        verify { activityWriter.set(room, activity) }
    }

    private fun withRoomExisting(exists: Boolean) {
        coEvery { chatRoomRepository.roomExists(room) } returns exists
    }

    private fun withDeclaredConfig(config: ChatRoomConfigModel?) {
        coEvery { declarationsRepository.get(room) } returns ChatRoomDeclarations.EMPTY.copy(config = config)
    }

    private fun assertUnknownRoom(result: Result<Unit>) {
        assertTrue("expected UnknownChatRoomException but was $result", result.exceptionOrNull() is UnknownChatRoomException)
    }
}
