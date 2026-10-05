package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.CreateRoomStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomResult
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import uniffi.truapi.ChatRoomRegistrationStatus

@OptIn(ExperimentalCoroutinesApi::class)
class TrUAPIWorkerChatTest {
    @Test
    fun `restored worker waits for the global chat extension before creating its initial room`() = runTest {
        val bridge = TrUAPIWorkerChat()
        val room = async { bridge.createRoom("inbox", "Inbox", "") }
        runCurrent()
        assertFalse(room.isCompleted)

        bridge.messaging = mockk<ProductChatMessaging> {
            coEvery { createRoom(any()) } returns Result.success(CreateProductRoomResult(CreateRoomStatus.New))
        }
        assertEquals(ChatRoomRegistrationStatus.NEW, room.await())
    }
}
