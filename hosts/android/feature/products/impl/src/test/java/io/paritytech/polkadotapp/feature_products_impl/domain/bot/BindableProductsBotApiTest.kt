package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.HostCallException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class BindableProductsBotApiTest {
    private val productId = ProductId.fromStoredValue("coinflip.dot")
    private val hostApiInteractor: HostApiInteractor = mockk()
    private val chatIdParam = ProductChatIdParameter("room")

    private fun bindable() = BindableProductsBotApi(hostApiInteractor, FixedProductId(productId))

    private class FakeMessaging(private val messageId: ChatMessageId) : ProductChatMessaging {
        override suspend fun createRoom(request: CreateProductRoomRequest): Result<CreateProductRoomResult> = TODO()
        override suspend fun sendMessage(chatIdParameter: ProductChatIdParameter, message: ProductBotMessage): Result<ChatMessageId> = Result.success(messageId)
        override fun subscribeChatRooms(): Flow<List<ProductChatRoom>> = flowOf(emptyList())
    }

    @Test
    fun `outgoing chat calls fail with messaging-not-supported when unbound`() = runTest {
        val api = bindable()

        val result = api.sendMessage(chatIdParam, ProductBotMessage.Text("hi"))

        assertTrue(result.isFailure)
        assertEquals("messaging_not_supported", (result.exceptionOrNull() as? HostCallException)?.code)
    }

    @Test
    fun `outgoing chat calls route to the bound target`() = runTest {
        val api = bindable()
        api.chatSlot.set(FakeMessaging("m1"))

        val result = api.sendMessage(chatIdParam, ProductBotMessage.Text("hi"))

        assertEquals("m1", result.getOrThrow())
    }

    @Test
    fun `unbind restores the messaging-not-supported behaviour`() = runTest {
        val api = bindable()
        api.chatSlot.set(FakeMessaging("m1"))
        api.chatSlot.clear()

        assertTrue(api.sendMessage(chatIdParam, ProductBotMessage.Text("hi")).isFailure)
    }
}
