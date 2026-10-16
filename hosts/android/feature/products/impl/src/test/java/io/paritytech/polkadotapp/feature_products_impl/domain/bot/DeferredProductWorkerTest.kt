package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RecordingWorker
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class DeferredProductWorkerTest {
    private val room = ProductChatIdParameter("lobby")

    @Test
    fun `a failed boot fails a user message instead of waiting for a worker`() = runTest {
        val worker = DeferredProductWorker()
        val cause = IllegalStateException("core runtime unavailable")
        worker.fail(cause)

        assertSame(cause, worker.onUserMessage(room, "hello").exceptionOrNull())
    }

    @Test
    fun `a failed boot reports the render instead of leaving the cell spinning`() = runTest {
        val worker = DeferredProductWorker()
        val cause = IllegalStateException("core runtime unavailable")
        worker.fail(cause)

        val result = worker.renderMessage(room, "m1", "type", DataByteArray.empty()).first()

        assertSame(cause, result.exceptionOrNull())
    }

    @Test
    fun `a caller cancelled while waiting unwinds instead of getting a failed Result`() = runTest {
        val worker = DeferredProductWorker()
        var returned = false

        val waiting = async { worker.onUserMessage(room, "hello").also { returned = true } }
        runCurrent()
        waiting.cancel()
        runCurrent()

        assertFalse("a cancelled wait must not return a value", returned)
    }

    @Test
    fun `an attached worker still serves calls`() = runTest {
        val worker = DeferredProductWorker()
        worker.attach(RecordingWorker())

        assertTrue(worker.onUserMessage(room, "hello").isSuccess)
    }
}
