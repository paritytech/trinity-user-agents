package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductRuntimeException
import kotlin.time.Duration.Companion.milliseconds

class RenderConnectRetryTest {
    private var opened = 0

    private fun renderThatConnectsAfter(failures: Int, error: () -> Throwable = { ProductRuntimeException.NotConnected() }) = flow {
        opened++
        if (opened <= failures) throw error()
        emit("face")
    }

    @Test
    fun `a render opened before the worker's client connects is retried until it does`() = runTest {
        val faces = renderThatConnectsAfter(failures = 2).retryWhileConnecting(attempts = 5, retryDelay = 100.milliseconds).toList()

        assertEquals(listOf("face"), faces)
        assertEquals(3, opened)
    }

    @Test
    fun `a worker that never connects stops being asked after the last attempt`() = runTest {
        val result = runCatching { renderThatConnectsAfter(failures = 10).retryWhileConnecting(attempts = 3, retryDelay = 100.milliseconds).toList() }

        assertTrue(result.exceptionOrNull() is ProductRuntimeException.NotConnected)
        assertEquals(4, opened)
    }

    @Test
    fun `any other render failure is not mistaken for a connection still opening`() = runTest {
        val result = runCatching {
            renderThatConnectsAfter(failures = 1) { IllegalStateException("renderer gone") }
                .retryWhileConnecting(attempts = 3, retryDelay = 100.milliseconds)
                .toList()
        }

        assertTrue(result.exceptionOrNull() is IllegalStateException)
        assertEquals(1, opened)
    }
}
