package io.paritytech.polkadotapp.common.data.worker.stateMachine

interface WorkerStateFactory<S> {
    fun createState(stateId: String, store: WorkerStateStore<S>): S?

    fun createDefaultState(): S
}
