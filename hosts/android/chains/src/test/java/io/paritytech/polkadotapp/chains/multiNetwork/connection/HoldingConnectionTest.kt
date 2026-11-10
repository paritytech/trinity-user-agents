package io.paritytech.polkadotapp.chains.multiNetwork.connection

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HoldingConnectionTest {
    private val refCounter = RealChainConnectionRefCounter()

    @Test
    fun `the connection is enabled while the flow is collected`() = runTest {
        var enabledWhileCollecting = false

        flowOf(1)
            .onEach { enabledWhileCollecting = isEnabled() }
            .holdingConnection(refCounter, CHAIN, LABEL)
            .toList()

        assertTrue(enabledWhileCollecting)
    }

    @Test
    fun `the connection is released once the flow completes`() = runTest {
        val collected = flowOf(1, 2).holdingConnection(refCounter, CHAIN, LABEL).toList()

        assertEquals(listOf(1, 2), collected)
        assertFalse(isEnabled())
    }

    /** A subscription never completes by itself, so cancellation is how it usually ends. */
    @Test
    fun `the connection is released once the collector is cancelled`() = runTest {
        val started = CompletableDeferred<Unit>()

        val job = flow<Int> {
            started.complete(Unit)
            awaitCancellation()
        }
            .holdingConnection(refCounter, CHAIN, LABEL)
            .launchIn(this)

        started.await()
        assertTrue(isEnabled())

        job.cancel()
        job.join()

        assertFalse(isEnabled())
    }

    private suspend fun isEnabled() = refCounter.shouldConnectionBeEnabled(CHAIN).first()

    private companion object {
        const val CHAIN = "chain"
        const val LABEL = "test"
    }
}
