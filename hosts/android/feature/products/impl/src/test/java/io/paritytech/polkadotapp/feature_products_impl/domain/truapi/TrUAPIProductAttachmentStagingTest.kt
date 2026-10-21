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
import uniffi.truapi.NativeChatStageRejection

class TrUAPIProductAttachmentStagingTest {
    private val productId = ProductId.fromStoredValue("sample.dot")
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
    fun `the core stages the bytes for the product and the room, and gives the key`() = runTest {
        val bytes = ByteArray(1_024) { 5 }
        coEvery { runtime.stageChatAttachment("sample.dot", "host", "image/png", bytes) } returns ByteArray(32) { 1 }

        val key = staging.stage(room, "image/png", bytes).getOrThrow()

        assertArrayEquals(ByteArray(32) { 1 }, key.value)
    }

    @Test
    fun `each rejection of the core becomes the same rejection of the chat`() = runTest {
        val rejections = listOf(
            NativeChatStageRejection.NotDeclared() to ProductAttachmentStageException.NotDeclared::class,
            NativeChatStageRejection.TooLarge(2_097_152u) to ProductAttachmentStageException.TooLarge::class,
            NativeChatStageRejection.PermissionDenied() to ProductAttachmentStageException.PermissionDenied::class,
            NativeChatStageRejection.AllowanceUnavailable("no slot") to ProductAttachmentStageException.AllowanceUnavailable::class,
            NativeChatStageRejection.SubmitFailed("timeout") to ProductAttachmentStageException.SubmitFailed::class,
        )

        for ((rejection, expected) in rejections) {
            coEvery { runtime.stageChatAttachment(any(), any(), any(), any()) } throws rejection

            val failure = staging.stage(room, "image/png", ByteArray(1)).exceptionOrNull()

            assertEquals(expected, failure!!::class)
        }
        assertEquals(2_097_152L, (NativeChatStageRejection.TooLarge(2_097_152u).toModel() as ProductAttachmentStageException.TooLarge).maxBytes)
    }

    @Test
    fun `a room of no product is not declared, and the core sees nothing`() = runTest {
        val failure = staging.stage(ChatId.forExtensionRoom("Other_bot", "host"), "image/png", ByteArray(1)).exceptionOrNull()

        assertTrue(failure is ProductAttachmentStageException.NotDeclared)
        verify(exactly = 0) { runtime.acquireWorker(any()) }
    }

    @Test
    fun `the worker is held while the block runs, and released after a failure`() = runTest {
        val outcome = runCatching { staging.holdingWorker(room) { error("upload failed") } }

        assertTrue(outcome.isFailure)
        verifyOrder {
            runtime.acquireWorker("sample.dot")
            runtime.releaseWorker("sample.dot")
        }
    }
}
