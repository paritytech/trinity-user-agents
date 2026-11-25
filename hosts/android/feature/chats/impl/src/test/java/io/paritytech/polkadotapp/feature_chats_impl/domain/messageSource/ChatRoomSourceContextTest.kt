package io.paritytech.polkadotapp.feature_chats_impl.domain.messageSource

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatRoomLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertSame
import org.junit.Test

/** The room screen holds the room while it shows it. */
class ChatRoomSourceContextTest {
    private val hostSource = mockk<ChatMessageSource>(relaxed = true)
    private val lifecycle = mockk<ChatRoomLifecycle> {
        coEvery { open(any()) } returns Result.success(Unit)
        coEvery { close(any()) } returns Unit
    }

    @Test
    fun `a room reads from the database`() = runBlocking<Unit> {
        assertSame(hostSource, context().source())
    }

    @Test
    fun `open and close reach the lifecycle once and in order`() = runBlocking<Unit> {
        val context = context()

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
    fun `a person chat holds nothing`() = runBlocking<Unit> {
        val personChat = ChatId.fromContact(ByteArray(32) { 1 }.toDataByteArray())
        val context = context(personChat)

        context.open()
        context.release()
        delay(500)

        coVerify(exactly = 0) { lifecycle.open(any()) }
    }

    @Test
    fun `a room that cannot open is unavailable and holds nothing`() = runBlocking<Unit> {
        coEvery { lifecycle.open(any()) } returns Result.failure(IllegalStateException("the worker did not start"))
        val context = context()

        context.open()
        context.close()

        coVerify(timeout = 5_000) { hostSource.markUnavailable() }
        coVerify(exactly = 0) { lifecycle.close(any()) }
    }

    @Test
    fun `a room that did not open opens again after a delay, with its newest page`() = runBlocking<Unit> {
        coEvery { lifecycle.open(any()) } returnsMany listOf(Result.failure(IllegalStateException("the worker did not start")), Result.success(Unit))
        val context = context()

        context.open()

        coVerify(timeout = 10_000) { hostSource.reload() }
        coVerifyOrder {
            lifecycle.open(productRoom)
            hostSource.markUnavailable()
            lifecycle.open(productRoom)
            hostSource.reload()
        }
    }

    @Test
    fun `a closed room does not open again`() = runBlocking<Unit> {
        coEvery { lifecycle.open(any()) } returns Result.failure(IllegalStateException("the worker did not start"))
        val context = context()

        context.open()
        coVerify(timeout = 5_000) { hostSource.markUnavailable() }
        context.close()
        delay(3_000)

        coVerify(exactly = 1) { lifecycle.open(productRoom) }
    }

    @Test
    fun `release closes an open room and stops the source`() = runBlocking<Unit> {
        val context = context()
        context.source()

        context.open()
        context.release()

        coVerify(timeout = 5_000) { lifecycle.close(productRoom) }
        coVerify(timeout = 5_000) { hostSource.close() }
    }

    private fun context(chatId: ChatId = productRoom): ChatRoomSourceContext {
        val services = mockk<ChatRoomServices> {
            coEvery { lifecycle(any()) } returns lifecycle
            every { hostStoreSource(any()) } returns hostSource
        }
        return ChatRoomSourceContext(chatId, services, Dispatchers.Default)
    }
}
