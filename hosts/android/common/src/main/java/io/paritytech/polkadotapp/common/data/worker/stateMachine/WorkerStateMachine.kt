@file:OptIn(kotlin.time.ExperimentalTime::class)

package io.paritytech.polkadotapp.common.data.worker.stateMachine

import androidx.work.ListenableWorker
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineState.*
import timber.log.Timber
import kotlin.time.Instant

interface WorkerStateMachine<S : WorkerStateMachineState<S, *>> {
    suspend fun createCurrentState(): Result<S>

    suspend fun performTransition(state: S): TransitionResult<S>
}

/**
 * Outcome of running a state machine to a stable point.
 * Driver can express a third semantic outcome — "the state wants the worker to retry at a precise
 * wall-clock epoch" — without using exceptions as control flow.
 */
sealed interface ExecutionOutcome {
    /** Reached a terminal state or stopped at a non-terminal state that cannot progress further now. */
    data object Done : ExecutionOutcome

    /** A state requested a precise reschedule at [epoch]. */
    data class WaitUntil(val epoch: Instant) : ExecutionOutcome

    /** A transition surfaced a real failure (RPC/network/signing). */
    data class Failure(val cause: Throwable) : ExecutionOutcome
}

fun ExecutionOutcome.toWorkerResult(retryOnFailure: Boolean = false): ListenableWorker.Result = when (this) {
    ExecutionOutcome.Done -> ListenableWorker.Result.success()
    is ExecutionOutcome.WaitUntil -> ListenableWorker.Result.retry()
    is ExecutionOutcome.Failure -> if (retryOnFailure) ListenableWorker.Result.retry() else ListenableWorker.Result.failure()
}

suspend fun <S : WorkerStateMachineState<S, *>> WorkerStateMachine<S>.executeUntilPossible(): ExecutionOutcome {
    var currentState: TransitionResult<S> =
        TransitionResult.TransitionPerformed(createCurrentState())

    while (currentState is TransitionResult.TransitionPerformed && currentState.outcome.isSuccess) {
        currentState = performTransition(currentState.outcome.getOrThrow())
    }

    return when (val finalState = currentState) {
        TransitionResult.StateTerminal -> ExecutionOutcome.Done
        is TransitionResult.WaitUntil -> ExecutionOutcome.WaitUntil(finalState.epoch)
        is TransitionResult.TransitionPerformed -> finalState.outcome.fold(
            onSuccess = { ExecutionOutcome.Done },
            onFailure = { ExecutionOutcome.Failure(it) },
        )
    }
}

abstract class BaseWorkerStateMachine<S : WorkerStateMachineState<S, T>, T>(
    private val localSession: WorkerStateMachineLocalSession<S>,
    private val stateFactory: WorkerStateFactory<S>,
) : WorkerStateMachine<S> {
    private val logger
        get() = Timber.tag(this::class.simpleName.orEmpty())

    abstract suspend fun createTransition(): T

    override suspend fun createCurrentState(): Result<S> {
        return runCatching {
            var currentState = localSession.getCurrentState()

            when (currentState) {
                null -> {
                    currentState = stateFactory.createDefaultState()
                    localSession.setCurrentState(currentState)
                    logger.d("No saved state found: Starting from determined initial state: ${currentState.id}")
                }

                is UnrecoverableFailureState<*> -> {
                    @Suppress("UNCHECKED_CAST")
                    val retryState = currentState.retryState as S

                    logger.d("Starting from unrecoverable error: ${currentState.format()}, restarting from ${retryState.format()}")
                    currentState = retryState
                    localSession.setCurrentState(currentState)
                }

                else -> {
                    logger.d("Starting from saved state: ${currentState.format()}")
                }
            }

            currentState
        }
    }

    override suspend fun performTransition(state: S): TransitionResult<S> {
        logger.d("Starting transition from: ${state.format()}")

        val transition = createTransition()
        val result = with(transition) {
            state.performTransition()
        }

        when (result) {
            is TransitionResult.TransitionPerformed -> {
                result.outcome.onSuccess { newState ->
                    localSession.setCurrentState(newState)
                    logger.d("Transitioned to ${newState.format()}")
                }.onFailure { throwable ->
                    logger.w(throwable, "Transition did not succeed")
                }
            }
            is TransitionResult.WaitUntil -> {
                logger.d("State requested precise wait until epoch=${result.epoch} from ${state.format()}")
            }
            TransitionResult.StateTerminal -> Unit
        }

        return result
    }

    private fun S.format(): String {
        val args = if (this is WithParams<*>) {
            "($params)"
        } else {
            ""
        }

        return id + args
    }
}
