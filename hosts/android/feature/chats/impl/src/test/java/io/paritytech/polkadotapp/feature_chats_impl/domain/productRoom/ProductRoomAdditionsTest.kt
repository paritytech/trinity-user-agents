package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.UnknownChatRoomException
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomCallWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomDeclarationsWriter
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRoomRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductRoomAdditionsTest {
    private val extensionId = "ProductBot_game.dot"
    private val room = ChatId.forExtensionRoom(extensionId, "lobby")

    private val chatRoomRepository: ChatRoomRepository = mockk()
    private val declarationsWriter: ChatRoomDeclarationsWriter = mockk(relaxed = true)
    private val callWriter: ChatRoomCallWriter = mockk(relaxed = true)

    private val additions = ProductRoomAdditions(
        chatRoomRepository = chatRoomRepository,
        declarationsWriter = declarationsWriter,
        callWriter = callWriter,
    )

    @Test
    fun `features of an own room are kept whole`() = runBlocking<Unit> {
        withRoomExisting(true)
        val features = ChatRoomFeaturesModel(calls = ChatRoomCallsModel(audio = true, video = false), hopAttachments = true)

        val result = additions.setFeatures(extensionId, room, features)

        assertTrue(result.isSuccess)
        coVerify { declarationsWriter.setFeatures(room, features) }
    }

    @Test
    fun `features for the room of another extension fail as an unknown room`() = runBlocking<Unit> {
        withRoomExisting(true)

        val result = additions.setFeatures("ProductBot_other.dot", room, ChatRoomFeaturesModel.HOST_DEFAULT)

        assertTrue("expected UnknownChatRoomException but was $result", result.exceptionOrNull() is UnknownChatRoomException)
        coVerify(exactly = 0) { declarationsWriter.setFeatures(any(), any()) }
    }

    @Test
    fun `a call is kept in memory and removed with null`() = runBlocking<Unit> {
        withRoomExisting(true)
        val call = ChatRoomCallModel.Ringing("c1", video = false, caller = "p1")

        additions.setCall(extensionId, room, call)
        additions.setCall(extensionId, room, null)

        verify { callWriter.set(room, call) }
        verify { callWriter.set(room, null) }
    }

    private fun withRoomExisting(exists: Boolean) {
        coEvery { chatRoomRepository.roomExists(room) } returns exists
    }
}
