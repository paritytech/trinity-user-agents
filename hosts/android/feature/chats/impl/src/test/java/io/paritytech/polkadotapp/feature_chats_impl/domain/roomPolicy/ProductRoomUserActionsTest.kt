package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductRoomUserActionsTest {
    private val roomId = ChatId.forExtensionRoom("ProductBot_weather", "berlin")

    private val querying = mockk<ProductChatQuerying>()
    private val extension = mockk<ChatExtension>(relaxUnitFun = true) {
        every { roomQuerying(any()) } returns querying
    }
    private val registry = mockk<ChatExtensionRegistry>()

    private val actions = ProductRoomUserActions(registry)

    @Test
    fun `an answer to the contact goes to the product as a user action`() = runBlocking<Unit> {
        withExtension()
        coEvery { querying.userAction(roomId, any()) } returns Result.success(ChatUserActionOutcome.Acknowledged)

        assertTrue(actions.contactAction(roomId, ChatUserActionModel.AcceptRequest).isSuccess)

        coVerify { querying.userAction(roomId, ChatUserActionModel.AcceptRequest) }
    }

    @Test
    fun `an answer in a room whose product keeps no messages fails`() = runBlocking<Unit> {
        withExtension()
        every { extension.roomQuerying(roomId) } returns null

        assertTrue(actions.contactAction(roomId, ChatUserActionModel.Block).isFailure)
    }

    private fun withExtension() {
        coEvery { registry.getExtensionForChat(roomId) } returns extension
    }
}
