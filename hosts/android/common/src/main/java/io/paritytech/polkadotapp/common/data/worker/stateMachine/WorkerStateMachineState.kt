@file:OptIn(kotlin.time.ExperimentalTime::class)

package io.paritytech.polkadotapp.common.data.worker.stateMachine

import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState.TransitionResult
import kotlin.time.Instant

interface WorkerStateMachineState<SELF : WorkerStateMachineState<SELF, TRANSITION>, TRANSITION> {
    val id: String

    interface WithParams<P> {
        val params: P
    }

    context(transition: TRANSITION)
    suspend fun performTransition(): TransitionResult<SELF>

    sealed class TransitionResult<out S : WorkerStateMachineState<out S, *>> {
        data object StateTerminal : TransitionResult<Nothing>()

        data class TransitionPerformed<S : WorkerStateMachineState<out S, *>>(
            val outcome: Result<S>
        ) : TransitionResult<S>()

        /**
         * State cannot make progress until [epoch] (wall-clock). Driver propagates this to the
         * worker as [ExecutionOutcome.WaitUntil] so the worker can reschedule itself with a precise
         * initial delay.
         */
        data class WaitUntil(val epoch: Instant) : TransitionResult<Nothing>()

        companion object {
            /** Shortcut for states that directly emit a failure outcome without the [TransitionPerformed] wrapping ceremony. */
            fun <S : WorkerStateMachineState<out S, *>> failure(cause: Throwable): TransitionPerformed<S> =
                TransitionPerformed(Result.failure(cause))
        }
    }

    interface UnrecoverableFailureState<S : WorkerStateMachineState<S, *>> {
        val retryState: S
    }
}

abstract class TerminalState<S : WorkerStateMachineState<S, T>, T> : WorkerStateMachineState<S, T> {
    context(transition: T)
    final override suspend fun performTransition(): TransitionResult<S> {
        return TransitionResult.StateTerminal
    }
}

abstract class NonTerminalState<S : WorkerStateMachineState<S, T>, T> : WorkerStateMachineState<S, T> {
    context(transition: T)
    protected abstract suspend fun performNonTerminalTransition(): Result<S>

    context(transition: T)
    final override suspend fun performTransition(): TransitionResult<S> {
        return TransitionResult.TransitionPerformed(performNonTerminalTransition())
    }
}
