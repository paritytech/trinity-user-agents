package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.ChatRoomUserStateWriter
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatRoomUserStatesTest {
    private val room = ChatId.forExtensionRoom("ProductBot_game.dot", "lobby")
    private val writer: ChatRoomUserStateWriter = mockk(relaxed = true)
    private val publisher: ChatRoomEventPublisher = mockk(relaxed = true)
    private val userStates = ChatRoomUserStates(writer, publisher)

    @Test
    fun `a new user state is stored, then the product hears of it`() = runBlocking<Unit> {
        val state = ChatRoomUserStateModel(mutedUntil = ChatRoomUserStateModel.MUTED_WITHOUT_END, archived = true)

        userStates.set(room, state)

        coVerifyOrder {
            writer.set(room, state)
            publisher.publish(room, ChatRoomEventModel.UserStateChanged(state))
        }
    }

    @Test
    fun `a person chat has no user state of the additions`() = runBlocking<Unit> {
        val personChat = ChatId.fromContact(ByteArray(32) { 1 }.toDataByteArray())

        val result = userStates.set(personChat, ChatRoomUserStateModel.NONE)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { writer.set(any(), any()) }
    }
}
