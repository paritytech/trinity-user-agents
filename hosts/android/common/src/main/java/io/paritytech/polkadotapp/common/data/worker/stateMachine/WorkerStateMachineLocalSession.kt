package io.paritytech.polkadotapp.common.data.worker.stateMachine

import kotlinx.coroutines.flow.Flow

interface WorkerStateMachineLocalSession<S> {
    suspend fun getCurrentState(): S?

    suspend fun setCurrentState(state: S)

    fun currentStateFlow(): Flow<S?>

    suspend fun resetState()
}
