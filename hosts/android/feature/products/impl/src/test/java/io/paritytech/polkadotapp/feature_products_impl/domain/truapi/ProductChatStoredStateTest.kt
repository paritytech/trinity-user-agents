package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBotPersonaRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatBotPersonaModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ChatRoomUserState

class ProductChatStoredStateTest {
    private val productId = ProductId.fromStoredValue("dim2.dot")
    private val extensionId = productId.toChatExtensionId()
    private val userStates = mockk<ChatRoomUserStateRepository>()
    private val personas = mockk<ChatBotPersonaRepository>()
    private val storedState = ProductChatStoredState(userStates, personas)

    @Test
    fun `the user states name only the rooms of the product, by their room ids`() = runTest {
        withUserStates(
            ChatId.forExtensionRoom(extensionId, "lobby") to ChatRoomUserStateModel(mutedUntil = 1_700_000_000_000, archived = false),
            ChatId.forExtensionRoom("ProductBot_other.dot", "lobby") to ChatRoomUserStateModel(mutedUntil = null, archived = true),
        )

        assertEquals(listOf(ChatRoomUserState("lobby", 1_700_000_000_000uL, archived = false)), storedState.roomUserStates(productId))
    }

    @Test
    fun `a mute without an end reaches the core as the largest u64`() = runTest {
        withUserStates(
            ChatId.forExtensionRoom(extensionId, "lobby") to
                ChatRoomUserStateModel(mutedUntil = ChatRoomUserStateModel.MUTED_WITHOUT_END, archived = true),
        )

        assertEquals(listOf(ChatRoomUserState("lobby", ULong.MAX_VALUE, archived = true)), storedState.roomUserStates(productId))
    }

    @Test
    fun `the bot ids are the ids of the personas of the product`() = runTest {
        every { personas.observe(extensionId) } returns flowOf(listOf(ChatBotPersonaModel("helper", "Helper", "data:image/png;base64,AA")))

        assertEquals(listOf("helper"), storedState.botIds(productId))
    }

    private fun withUserStates(vararg states: Pair<ChatId, ChatRoomUserStateModel>) {
        every { userStates.observeAll() } returns flowOf(states.toMap())
    }
}
