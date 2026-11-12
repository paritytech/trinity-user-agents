package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.novasama.substrate_sdk_android.wsrpc.state.SocketStateMachine.State
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.seconds

class ShortLivedConnectionDetectorTest {
    private val config = ShortLivedConnectionConfig(
        maxShortLife = 5.seconds,
        dropsBeforeSwitch = 2,
    )

    @Test
    fun `two short-lived connections in a row on one node request a switch`() = runBlocking<Unit> {
        val events = detect {
            connected(NODE_A, at = 0)
            dropped(at = 100)
            connected(NODE_A, at = 500)
            dropped(at = 600)
        }

        assertEquals(1, events)
    }

    @Test
    fun `a single short-lived connection is tolerated`() = runBlocking<Unit> {
        val events = detect {
            connected(NODE_A, at = 0)
            dropped(at = 100)
            connected(NODE_A, at = 500)
        }

        assertEquals(0, events)
    }

    @Test
    fun `a connection that outlived the threshold resets the count`() = runBlocking<Unit> {
        val events = detect {
            connected(NODE_A, at = 0)
            dropped(at = 100)
            connected(NODE_A, at = 500)
            dropped(at = 10_000)
            connected(NODE_A, at = 10_500)
            dropped(at = 10_600)
        }

        assertEquals(0, events)
    }

    @Test
    fun `failures while connecting are left to the SDK attempt counter`() = runBlocking<Unit> {
        val events = detect {
            connecting(NODE_A, at = 0)
            dropped(at = 100)
            connecting(NODE_A, at = 400)
            dropped(at = 500)
            connecting(NODE_A, at = 1_100)
            dropped(at = 1_200)
        }

        assertEquals(0, events)
    }

    @Test
    fun `a switch to another node starts the count over`() = runBlocking<Unit> {
        val events = detect {
            connected(NODE_A, at = 0)
            dropped(at = 100)
            connected(NODE_B, at = 500)
            dropped(at = 600)
        }

        assertEquals(0, events)
    }

    @Test
    fun `pausing a live connection is not a drop`() = runBlocking<Unit> {
        val events = detect {
            connected(NODE_A, at = 0)
            paused(NODE_A, at = 100)
            connected(NODE_A, at = 500)
            dropped(at = 600)
        }

        assertEquals(0, events)
    }

    @Test
    fun `the count starts over after a switch was requested`() = runBlocking<Unit> {
        val events = detect {
            connected(NODE_A, at = 0)
            dropped(at = 100)
            connected(NODE_A, at = 500)
            dropped(at = 600)
            connected(NODE_A, at = 1_000)
            dropped(at = 1_100)
        }

        assertEquals(1, events)
    }

    private var nowMillis = 0L

    private suspend fun detect(script: suspend SocketScript.() -> Unit): Int {
        val detector = ShortLivedConnectionDetector(config, FakeTimeProvider { nowMillis })
        val states = flow { SocketScript(this::emit).script() }

        return detector.nodeChangeEvents(states).toList().size
    }

    private inner class SocketScript(private val emit: suspend (State) -> Unit) {
        suspend fun connected(url: String, at: Long) = emitAt(at, connectedTo(url))

        suspend fun connecting(url: String, at: Long) = emitAt(at, State.Connecting(url = url))

        suspend fun paused(url: String, at: Long) = emitAt(at, State.Paused(url = url))

        suspend fun dropped(at: Long) = emitAt(at, State.WaitingForReconnect(url = "", pendingSendables = emptySet()))

        private suspend fun emitAt(at: Long, state: State) {
            nowMillis = at
            emit(state)
        }
    }

    private fun connectedTo(url: String) = State.Connected(
        url = url,
        toResendOnReconnect = emptySet(),
        unknownSubscriptionResponses = emptyMap(),
        waitingForResponse = emptyMap(),
        subscriptions = emptySet(),
    )

    private companion object {
        const val NODE_A = "wss://node-a.test"
        const val NODE_B = "wss://node-b.test"
    }
}
