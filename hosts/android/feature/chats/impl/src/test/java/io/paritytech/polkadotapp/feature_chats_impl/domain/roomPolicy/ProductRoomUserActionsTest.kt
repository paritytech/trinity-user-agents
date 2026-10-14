package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomUserEvent
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionOutcome
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatMessageRepository
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
    private val repository = mockk<ChatMessageRepository>(relaxUnitFun = true)

    private val actions = ProductRoomUserActions(registry, repository)

    private val productSourceRoom = ChatRoomPolicy.Defaults.copy(messageStore = ChatRoomPolicy.MessageStore.PRODUCT)

    @Test
    fun `a button press reaches the product as an event`() = runBlocking<Unit> {
        withExtension()

        assertTrue(actions.sendEvent(roomId, ChatRoomUserEvent.ActionTriggered("message-1", "yes")).isSuccess)

        coVerify { extension.onUserEvent(roomId, ChatRoomUserEvent.ActionTriggered("message-1", "yes")) }
    }

    @Test
    fun `a room that no extension serves fails the event`() = runBlocking<Unit> {
        coEvery { registry.getExtensionForChat(roomId) } returns null

        assertTrue(actions.sendEvent(roomId, ChatRoomUserEvent.ActionTriggered("message-1", "yes")).isFailure)
    }

    @Test
    fun `a delete in a host-store room removes the message and tells the product`() = runBlocking<Unit> {
        withExtension()

        assertTrue(actions.deleteMessage(roomId, "message-1", ChatRoomPolicy.Defaults).isSuccess)

        coVerify { repository.removeMessage("message-1") }
        coVerify { extension.onUserEvent(roomId, ChatRoomUserEvent.MessageDeleted("message-1")) }
    }

    @Test
    fun `a delete in a product-source room goes to the product as a user action`() = runBlocking<Unit> {
        withExtension()
        coEvery { querying.userAction(roomId, any()) } returns Result.success(ChatUserActionOutcome.Acknowledged)

        assertTrue(actions.deleteMessage(roomId, "message-1", productSourceRoom).isSuccess)

        coVerify { querying.userAction(roomId, ChatUserActionModel.DeleteMessage("message-1")) }
        coVerify(exactly = 0) { repository.removeMessage(any()) }
    }

    @Test
    fun `a retry goes to the product as a user action`() = runBlocking<Unit> {
        withExtension()
        coEvery { querying.userAction(roomId, any()) } returns Result.success(ChatUserActionOutcome.Acknowledged)

        assertTrue(actions.retry(roomId, "message-1").isSuccess)

        coVerify { querying.userAction(roomId, ChatUserActionModel.Retry("message-1")) }
    }

    @Test
    fun `a retry in a room whose product keeps no messages fails`() = runBlocking<Unit> {
        withExtension()
        every { extension.roomQuerying(roomId) } returns null

        assertTrue(actions.retry(roomId, "message-1").isFailure)
    }

    private fun withExtension() {
        coEvery { registry.getExtensionForChat(roomId) } returns extension
    }
}
