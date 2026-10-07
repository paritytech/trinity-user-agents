package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.RuntimeState
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.wasmi_worker.WasmiSandboxError
import io.paritytech.polkadotapp.wasmi_worker.WasmiTurn
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.WsBridgeEndpoint

/**
 * Turn ordering, lifecycle mapping and failure handling, with a scripted guest and an in-memory
 * bridge in place of the native sandbox and the loopback socket.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class WasmiWorkerRuntimeTest {
    private val dispatcher = StandardTestDispatcher()
    private val lifecycle = MutableSharedFlow<AppLifecycleState>(extraBufferCapacity = 8)
    private val lifecycleObserver = object : AppLifecycleObserver {
        override fun subscribe(): Flow<AppLifecycleState> = lifecycle
        override fun getCurrentState() = AppLifecycleState.FOREGROUND
    }
    private val bridge = FakeBridge()
    private val endpoint = WsBridgeEndpoint(port = 9731u, token = "t")

    private fun TestScope.runtime() = WasmiWorkerRuntime(
        parentScope = this,
        appLifecycleObserver = lifecycleObserver,
        connector = bridge,
        dispatchers = TestCoroutineDispatchers(dispatcher),
    )

    @Test
    fun `start runs the start turn, sends its frames, and reports ready`() = runTest(dispatcher) {
        val guest = ScriptedGuest { turn -> if (turn is WasmiTurn.Start) listOf(byteArrayOf(1), byteArrayOf(2)) else emptyList() }
        val runtime = runtime()

        val started = runtime.start(endpoint, guest)
        advanceUntilIdle()

        assertTrue(started.isSuccess)
        assertEquals(RuntimeState.Ready, runtime.state.value)
        assertEquals(listOf(listOf<Byte>(1), listOf<Byte>(2)), bridge.sent.map { it.toList() })
        assertEquals(listOf("Start"), guest.turns.map { it.name() })
        runtime.dispose()
    }

    @Test
    fun `bridge frames keep their order and lifecycle events become suspend and resume turns`() = runTest(dispatcher) {
        val guest = ScriptedGuest { emptyList() }
        val runtime = runtime()
        runtime.start(endpoint, guest)
        advanceUntilIdle()

        bridge.events.onFrame(byteArrayOf(7))
        bridge.events.onFrame(byteArrayOf(8))
        advanceUntilIdle()
        lifecycle.tryEmit(AppLifecycleState.BACKGROUND)
        advanceUntilIdle()
        lifecycle.tryEmit(AppLifecycleState.FOREGROUND)
        advanceUntilIdle()

        assertEquals(listOf("Start", "Frame(7)", "Frame(8)", "Suspend", "Resume"), guest.turns.map { it.name() })
        assertEquals(1, guest.maxConcurrent)
        runtime.dispose()
    }

    @Test
    fun `a guest fault ends the runtime with the cause and stops draining`() = runTest(dispatcher) {
        val guest = ScriptedGuest { turn ->
            if (turn is WasmiTurn.Frame) throw WasmiSandboxError.Faulted(IllegalStateException("ran out of fuel"))
            emptyList()
        }
        val runtime = runtime()
        runtime.start(endpoint, guest)
        advanceUntilIdle()

        bridge.events.onFrame(byteArrayOf(1))
        bridge.events.onFrame(byteArrayOf(2))
        advanceUntilIdle()

        val state = runtime.state.value
        assertTrue(state.toString(), state is RuntimeState.Error && state.cause.contains("ran out of fuel"))
        assertEquals(listOf("Start", "Frame(1)"), guest.turns.map { it.name() })
        runtime.dispose()
    }

    @Test
    fun `a bridge that closes ends the runtime and a start that faults fails the start`() = runTest(dispatcher) {
        val closing = runtime()
        closing.start(endpoint, ScriptedGuest { emptyList() })
        advanceUntilIdle()
        bridge.events.onClosed(1006, "gone")
        assertTrue(closing.state.value is RuntimeState.Error)
        closing.dispose()

        val faulting = runtime()
        val started = faulting.start(
            endpoint,
            ScriptedGuest { throw WasmiSandboxError.Faulted(IllegalStateException("no `on_start`")) },
        )
        advanceUntilIdle()
        assertTrue(started.exceptionOrNull() is WasmiRuntimeError.StartFailed)
        faulting.dispose()
    }

    @Test
    fun `dispose destroys the guest, closes the bridge, and drops later frames`() = runTest(dispatcher) {
        val guest = ScriptedGuest { emptyList() }
        val runtime = runtime()
        runtime.start(endpoint, guest)
        advanceUntilIdle()

        runtime.dispose()
        bridge.events.onFrame(byteArrayOf(9))
        advanceUntilIdle()

        assertTrue(guest.destroyed)
        assertTrue(bridge.closed)
        assertEquals(listOf("Start"), guest.turns.map { it.name() })
    }

    private class ScriptedGuest(private val script: (WasmiTurn) -> List<ByteArray>) : WorkerGuest {
        val turns = mutableListOf<WasmiTurn>()
        var destroyed = false
        var maxConcurrent = 0
        private var inFlight = 0

        override fun turn(turn: WasmiTurn): Result<List<ByteArray>> {
            inFlight += 1
            maxConcurrent = maxOf(maxConcurrent, inFlight)
            turns += turn
            return runCatching { script(turn) }.also { inFlight -= 1 }
        }

        override fun takeLogs(): List<String> = emptyList()

        override fun destroy() {
            destroyed = true
        }
    }

    private class FakeBridge : WorkerBridgeConnector {
        lateinit var events: WorkerBridgeEvents
        val sent = mutableListOf<ByteArray>()
        var closed = false

        override fun connect(endpoint: WsBridgeEndpoint, events: WorkerBridgeEvents): WorkerBridgeConnection {
            this.events = events
            return object : WorkerBridgeConnection {
                override fun send(frame: ByteArray) {
                    sent += frame
                }

                override fun close() {
                    closed = true
                }
            }
        }
    }

    private fun WasmiTurn.name(): String = when (this) {
        WasmiTurn.Start -> "Start"
        is WasmiTurn.Frame -> "Frame(${bytes.joinToString()})"
        WasmiTurn.Suspend -> "Suspend"
        WasmiTurn.Resume -> "Resume"
    }
}
