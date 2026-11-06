package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.novasama.substrate_sdk_android.wsrpc.state.SocketStateMachine.State
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class FailedNodeSwitchesTest {
    @Test
    fun `a node is switched after three failed attempts`() = runBlocking<Unit> {
        val switches = switchesAt(nodes = 2) {
            failAttempts(NODE_A, upTo = 3)
        }

        assertEquals(listOf(3), switches)
    }

    @Test
    fun `every round over dead nodes gives each node one more attempt`() = runBlocking<Unit> {
        val switches = switchesAt(nodes = 2) {
            repeat(6) { round -> failAttempts(if (round % 2 == 0) NODE_A else NODE_B, upTo = 10) }
        }

        assertEquals(listOf(3, 3, 4, 4, 5, 5), switches)
    }

    @Test
    fun `the extra attempts stop growing at the cap`() = runBlocking<Unit> {
        val switches = switchesAt(nodes = 1) {
            repeat(10) { failAttempts(NODE_A, upTo = 20) }
        }

        val cap = NODE_SWITCH_ATTEMPT_THRESHOLD + MAX_EXTRA_ATTEMPTS_BEFORE_SWITCH
        assertEquals(listOf(3, 4, 5, 6, 7, 8, cap, cap, cap, cap), switches)
    }

    @Test
    fun `a connection that comes up starts the rounds over`() = runBlocking<Unit> {
        val switches = switchesAt(nodes = 1) {
            failAttempts(NODE_A, upTo = 10)
            failAttempts(NODE_A, upTo = 10)
            connected(NODE_A)
            failAttempts(NODE_A, upTo = 10)
        }

        assertEquals(listOf(3, 4, 3), switches)
    }

    /** The attempt at which each switch was asked for. A script stops a node's attempts at the first switch, as switchUrl does. */
    private suspend fun switchesAt(nodes: Int, script: suspend SocketScript.() -> Unit): List<Int> {
        val script = SocketScript()
        val states = flow { script.emitTo = { emit(it) }; script.script() }

        val switches = mutableListOf<Int>()
        states.failedNodeSwitches { nodes }.collect {
            switches += script.lastAttempt
            script.switched = true
        }
        return switches
    }

    private class SocketScript {
        lateinit var emitTo: suspend (State) -> Unit
        var lastAttempt = -1
        var switched = false

        suspend fun failAttempts(url: String, upTo: Int) {
            switched = false
            for (attempt in 0..upTo) {
                lastAttempt = attempt
                emitTo(State.WaitingForReconnect(url = url, attempt = attempt, pendingSendables = emptySet()))
                if (switched) return
            }
        }

        suspend fun connected(url: String) = emitTo(
            State.Connected(
                url = url,
                toResendOnReconnect = emptySet(),
                unknownSubscriptionResponses = emptyMap(),
                waitingForResponse = emptyMap(),
                subscriptions = emptySet(),
            )
        )
    }

    private companion object {
        const val NODE_A = "wss://node-a.test"
        const val NODE_B = "wss://node-b.test"
    }
}
