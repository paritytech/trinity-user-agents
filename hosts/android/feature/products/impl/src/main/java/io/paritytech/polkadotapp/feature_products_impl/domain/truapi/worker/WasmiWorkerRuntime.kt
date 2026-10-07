package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.RuntimeState
import io.paritytech.polkadotapp.wasmi_worker.WasmiTurn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import uniffi.truapi.WsBridgeEndpoint

sealed class WasmiRuntimeError(message: String) : Exception(message) {
    class BridgeClosed(code: Int, reason: String) : WasmiRuntimeError("worker bridge closed ($code): $reason")
    class BridgeFailed(cause: Throwable) : WasmiRuntimeError("worker bridge failed: ${cause.message}")
    class StartFailed(cause: String) : WasmiRuntimeError("wasmi worker did not start: $cause")
    object Disposed : WasmiRuntimeError("wasmi worker runtime disposed")
}

/** The guest instance the runtime drives; the real one wraps the native sandbox. */
interface WorkerGuest {
    fun turn(turn: WasmiTurn): Result<List<ByteArray>>
    fun takeLogs(): List<String>
    fun destroy()
}

/**
 * Runs one wasm Worker product under the embedded sandbox and relays its frames over the
 * execution's loopback bridge, in place of the hidden WebView.
 *
 * One VM dispatcher owns the guest: bridge frames and lifecycle events arrive on other threads and
 * are queued as turns, so the guest never runs re-entrantly and a turn's outgoing frames are sent
 * in order before the next turn starts. A faulted guest ends the runtime: the sandbox stays poisoned,
 * [state] reports the cause, and nothing reconnects or reboots on its own. The supervisor decides.
 */
class WasmiWorkerRuntime @AssistedInject constructor(
    @Assisted parentScope: CoroutineScope,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val connector: WorkerBridgeConnector,
    private val dispatchers: CoroutineDispatchers,
) {
    // Owned, so dispose() can cancel the pump and the lifecycle subscription without touching the
    // worker's scope, which the supervisor cancels on its own schedule.
    private val scope = parentScope.childScope()

    @AssistedFactory
    interface Factory {
        fun create(scope: CoroutineScope): WasmiWorkerRuntime
    }

    private val mutableState = MutableStateFlow<RuntimeState>(RuntimeState.NotInitialized)
    val state: StateFlow<RuntimeState> = mutableState

    private val turns = Channel<WasmiTurn>(Channel.UNLIMITED)
    private var guest: WorkerGuest? = null
    private var connection: WorkerBridgeConnection? = null
    private var disposed = false

    /**
     * Connects to the execution's bridge, runs the guest's start turn, and resolves once that turn
     * ran. Frames that arrive before the start turn queue behind it.
     */
    suspend fun start(endpoint: WsBridgeEndpoint, guest: WorkerGuest): Result<Unit> {
        if (disposed) return Result.failure(WasmiRuntimeError.Disposed)
        this.guest = guest
        connection = connector.connect(endpoint, bridgeEvents)
        scope.launch(dispatchers.io.limitedParallelism(1)) { drainTurns() }
        observeLifecycle()
        turns.send(WasmiTurn.Start)
        return awaitRunning().onFailure { fail(it) }
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        turns.close()
        scope.cancel()
        connection?.close()
        connection = null
        guest?.destroy()
        guest = null
    }

    private val bridgeEvents = object : WorkerBridgeEvents {
        override fun onFrame(frame: ByteArray) {
            if (!disposed) turns.trySend(WasmiTurn.Frame(frame))
        }

        override fun onClosed(code: Int, reason: String) = fail(WasmiRuntimeError.BridgeClosed(code, reason))

        override fun onFailure(cause: Throwable) = fail(WasmiRuntimeError.BridgeFailed(cause))
    }

    private suspend fun drainTurns() {
        for (turn in turns) {
            val current = guest ?: return
            val outcome = current.turn(turn)
            current.takeLogs().forEach { Timber.d("wasmi guest: %s", it) }
            outcome
                .onSuccess { frames ->
                    frames.forEach { connection?.send(it) }
                    if (turn is WasmiTurn.Start) mutableState.compareAndSet(RuntimeState.NotInitialized, RuntimeState.Ready)
                }
                .onFailure {
                    fail(it)
                    return
                }
        }
    }

    private fun observeLifecycle() {
        scope.launch {
            appLifecycleObserver.subscribe().collect { lifecycle ->
                if (disposed) return@collect
                when (lifecycle) {
                    AppLifecycleState.BACKGROUND -> turns.trySend(WasmiTurn.Suspend)
                    AppLifecycleState.FOREGROUND -> turns.trySend(WasmiTurn.Resume)
                }
            }
        }
    }

    private suspend fun awaitRunning(): Result<Unit> =
        when (val settled = mutableState.first { it !is RuntimeState.NotInitialized }) {
            is RuntimeState.Ready -> Result.success(Unit)
            is RuntimeState.Error -> Result.failure(WasmiRuntimeError.StartFailed(settled.cause))
            RuntimeState.NotInitialized -> Result.failure(WasmiRuntimeError.Disposed)
        }

    private fun fail(cause: Throwable) {
        mutableState.update { if (it is RuntimeState.Error) it else RuntimeState.Error(cause.message ?: cause.javaClass.simpleName) }
    }
}
