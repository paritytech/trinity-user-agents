package io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine

import com.google.gson.Gson
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ContainerBridgeTest {
    @Test
    fun `failing subscription reports the error to JS without escaping to the scope`() = runTest {
        val uncaught = mutableListOf<Throwable>()
        val scope = CoroutineScope(
            SupervisorJob() + UnconfinedTestDispatcher(testScheduler) + CoroutineExceptionHandler { _, e -> uncaught += e }
        )
        val scripts = mutableListOf<String>()
        val incoming = slot<(String) -> Unit>()
        val transport = mockk<ContainerTransport> {
            every { registerIncomingHandler(capture(incoming)) } returns Unit
            coEvery { evaluateJs(capture(scripts)) } returns Unit
        }
        val bridge = ContainerBridge(transport, scope, Gson())
        bridge.registerSubscription<Unit, String>("failing") {
            flow { throw HostCallException("NotFound", "no payment found") }
        }

        incoming.captured("""{"type":"subscribe","id":"s1","method":"failing","params":{}}""")

        assertTrue(uncaught.isEmpty())
        assertEquals(1, scripts.size)
        assertTrue(scripts.single().contains("NotFound"))
    }
}
