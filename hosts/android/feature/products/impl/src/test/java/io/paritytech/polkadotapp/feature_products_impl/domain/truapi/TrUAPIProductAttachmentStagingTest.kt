package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.mockk.verifyOrder
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductAttachmentStageException
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerExecutionState
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatResource
import uniffi.truapi.NativeChatStageRejection

class TrUAPIProductAttachmentStagingTest {
    private val productId = ProductId.fromStoredValue("chat.dot")
    private val room = ChatId.forExtensionRoom(productId.toChatExtensionId(), "host")
    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
    private val runtimeProvider = mockk<TrUAPIHostRuntimeProvider> {
        coEvery { runtime() } returns Result.success(runtime)
    }
    private val workers = mockk<TrUAPIWorkerSupervisor> {
        every { executionState(productId) } returns flowOf(WorkerExecutionState.Running(mockk<TrUAPIProductExecution>()))
    }
    private val staging = TrUAPIProductAttachmentStaging(Lazy { runtimeProvider }, Lazy { workers })

    @Test
    fun `the core uploads the bytes of the room to HOP and gives the file`() = runTest {
        val bytes = ByteArray(1_024) { 5 }
        coEvery { runtime.hopChatAttachment("chat.dot", "host", bytes, any()) } returns
            ChatResource.Hop(ByteArray(32) { 1 }, ByteArray(32) { 2 }, "wss://hop.example", 1_024uL)

        val file = staging.uploadToHop(room, bytes).getOrThrow()

        assertArrayEquals(ByteArray(32) { 1 }, file.identifier.value)
        assertArrayEquals(ByteArray(32) { 2 }, file.ticket.value)
        assertEquals("wss://hop.example", file.node)
        assertEquals(1_024L, file.size)
    }

    @Test
    fun `the upload reports the bytes that the node holds, as the core gives them`() = runTest {
        val bytes = ByteArray(2_048) { 5 }
        coEvery { runtime.hopChatAttachment("chat.dot", "host", bytes, any()) } answers {
            val progress = requireNotNull(arg<((ULong, ULong) -> Unit)?>(3))
            progress(0uL, 2_048uL)
            progress(1_024uL, 2_048uL)
            progress(2_048uL, 2_048uL)
            ChatResource.Hop(ByteArray(32) { 1 }, ByteArray(32) { 2 }, "wss://hop.example", 2_048uL)
        }
        val reports = mutableListOf<Pair<Long, Long>>()

        staging.uploadToHop(room, bytes) { sent, total -> reports += sent to total }.getOrThrow()

        assertEquals(listOf(0L to 2_048L, 1_024L to 2_048L, 2_048L to 2_048L), reports)
    }

    @Test
    fun `each rejection of the core becomes the same rejection of the chat`() = runTest {
        val rejections = listOf(
            NativeChatStageRejection.NotDeclared() to ProductAttachmentStageException.NotDeclared::class,
            NativeChatStageRejection.TooLarge(2_097_152u) to ProductAttachmentStageException.TooLarge::class,
            NativeChatStageRejection.SubmitFailed("timeout") to ProductAttachmentStageException.SubmitFailed::class,
        )

        for ((rejection, expected) in rejections) {
            coEvery { runtime.hopChatAttachment(any(), any(), any(), any()) } throws rejection

            val failure = staging.uploadToHop(room, ByteArray(1)).exceptionOrNull()

            assertEquals(expected, failure!!::class)
        }
        assertEquals(2_097_152L, (NativeChatStageRejection.TooLarge(2_097_152u).toModel() as ProductAttachmentStageException.TooLarge).maxBytes)
    }

    @Test
    fun `a room of no product is not declared, and the core sees nothing`() = runTest {
        val failure = staging.uploadToHop(ChatId.forExtensionRoom("Other_bot", "host"), ByteArray(1)).exceptionOrNull()

        assertTrue(failure is ProductAttachmentStageException.NotDeclared)
        verify(exactly = 0) { runtime.acquireWorker(any()) }
    }

    @Test
    fun `the worker is held while the block runs, and released after a failure`() = runTest {
        val outcome = runCatching { staging.holdingWorker(room) { error("upload failed") } }

        assertTrue(outcome.isFailure)
        verifyOrder {
            runtime.acquireWorker("chat.dot")
            runtime.releaseWorker("chat.dot")
        }
    }
}
