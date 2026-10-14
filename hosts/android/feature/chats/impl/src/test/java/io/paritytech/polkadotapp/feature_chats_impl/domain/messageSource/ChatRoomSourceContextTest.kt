package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomLifecycle
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

/** The room screen picks the message source from the policy and holds the room while it shows it (P8.5). */
class ChatRoomSourceContextTest {
    private val hostSource = mockk<ChatMessageSource>(relaxed = true)
    private val productSource = mockk<ChatMessageSource>(relaxed = true)
    private val querying = mockk<ProductChatQuerying>()
    private val lifecycle = mockk<ChatRoomLifecycle> {
        coEvery { open(any()) } returns Result.success(Unit)
        coEvery { close(any()) } returns Unit
    }
    private val productSourceFactory = ProductMessageSourceFactory { _, _, _ -> productSource }

    @Test
    fun `a product-source room reads from the product`() = runBlocking<Unit> {
        val context = context(policy(ChatRoomPolicy.MessageStore.PRODUCT))

        assertSame(productSource, context.source())
        assertSame(productSource, context.productSource())
    }

    @Test
    fun `a host-store room reads from the database`() = runBlocking<Unit> {
        val context = context(policy(ChatRoomPolicy.MessageStore.HOST))

        assertSame(hostSource, context.source())
        assertNull(context.productSource())
    }

    @Test
    fun `a person chat reads from the database`() = runBlocking<Unit> {
        val personChat = ChatId.fromContact(ByteArray(32) { 1 }.toDataByteArray())
        val context = context(policy(ChatRoomPolicy.MessageStore.PRODUCT), chatId = personChat)

        assertSame(hostSource, context.source())
    }

    @Test
    fun `S24 open and close reach the lifecycle once and in order`() = runBlocking<Unit> {
        val context = context(policy(ChatRoomPolicy.MessageStore.HOST))

        context.open()
        context.open()
        context.close()
        context.close()

        coVerify(timeout = 5_000) { lifecycle.close(productRoom) }
        coVerifyOrder {
            lifecycle.open(productRoom)
            lifecycle.close(productRoom)
        }
        coVerify(exactly = 1) { lifecycle.open(any()) }
        coVerify(exactly = 1) { lifecycle.close(any()) }
    }

    @Test
    fun `a room that cannot open is unavailable and holds nothing`() = runBlocking<Unit> {
        coEvery { lifecycle.open(any()) } returns Result.failure(IllegalStateException("the worker did not start"))
        val context = context(policy(ChatRoomPolicy.MessageStore.PRODUCT))

        context.open()
        context.close()

        coVerify(timeout = 5_000) { productSource.markUnavailable() }
        coVerify(exactly = 0) { lifecycle.close(any()) }
    }

    @Test
    fun `retry opens the room and asks for the newest page`() = runBlocking<Unit> {
        val context = context(policy(ChatRoomPolicy.MessageStore.PRODUCT))

        context.retry()

        coVerify(timeout = 5_000) { productSource.reload() }
        coVerifyOrder {
            lifecycle.open(productRoom)
            productSource.reload()
        }
    }

    @Test
    fun `release closes an open room and stops the source`() = runBlocking<Unit> {
        val context = context(policy(ChatRoomPolicy.MessageStore.PRODUCT))
        context.source()

        context.open()
        context.release()

        coVerify(timeout = 5_000) { lifecycle.close(productRoom) }
        coVerify(timeout = 5_000) { productSource.close() }
    }

    private fun context(policy: ChatRoomPolicy, chatId: ChatId = productRoom): ChatRoomSourceContext {
        val services = mockk<ChatRoomServices> {
            coEvery { policy(any()) } returns policy
            coEvery { lifecycle(any()) } returns lifecycle
            coEvery { querying(any()) } returns querying
            every { hostStoreSource(any()) } returns hostSource
        }
        return ChatRoomSourceContext(chatId, services, productSourceFactory, Dispatchers.Default)
    }

    private fun policy(store: ChatRoomPolicy.MessageStore) = ChatRoomPolicy.Defaults.copy(messageStore = store)
}
