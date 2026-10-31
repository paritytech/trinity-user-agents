package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.novasama.substrate_sdk_android.wsrpc.state.SocketStateMachine.State
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

internal const val NODE_SWITCH_ATTEMPT_THRESHOLD = 3

// Seven SDK attempts on one node: its last backoff is 300 ms * 2^7, about 38 s.
internal const val MAX_EXTRA_ATTEMPTS_BEFORE_SWITCH = 5

/**
 * Asks for a node switch once the SDK has retried the current node enough times.
 *
 * [io.novasama.substrate_sdk_android.wsrpc.SocketService.switchUrl] resets the SDK's attempt
 * counter, and with it its exponential backoff (300 ms base, base 2). With a fixed threshold, a
 * chain whose every node is down would rotate forever at the backoff's floor: about two attempts
 * a second for as long as the app runs. So each full round of failed switches lets every node
 * take one more attempt before the next switch, up to [MAX_EXTRA_ATTEMPTS_BEFORE_SWITCH], and the
 * backoff keeps growing. A connection that comes up starts the count over.
 */
internal fun Flow<State>.failedNodeSwitches(nodeCount: () -> Int): Flow<Unit> {
    val states = this
    return flow {
        var failedSwitches = 0

        states.collect { state ->
            when (state) {
                is State.Connected -> failedSwitches = 0

                is State.WaitingForReconnect -> {
                    val rounds = failedSwitches / nodeCount().coerceAtLeast(1)
                    val threshold = NODE_SWITCH_ATTEMPT_THRESHOLD + rounds.coerceAtMost(MAX_EXTRA_ATTEMPTS_BEFORE_SWITCH)

                    if (state.attempt >= threshold) {
                        failedSwitches++
                        emit(Unit)
                    }
                }

                else -> Unit
            }
        }
    }
}
