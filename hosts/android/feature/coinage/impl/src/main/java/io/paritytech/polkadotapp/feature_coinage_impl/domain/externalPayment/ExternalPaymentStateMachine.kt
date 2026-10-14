package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.data.worker.stateMachine.BaseWorkerStateMachine
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateFactory
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateStore
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.ExternalPaymentState

/**
 * State factory for [ExternalPaymentStateMachine]. Throws: payment rows are always
 * inserted before the worker is enqueued, so getCurrentState() never returns null;
 * if somehow it does, restarting from a synthesized default is unsafe.
 */
class ExternalPaymentStateFactory : WorkerStateFactory<ExternalPaymentState> {
    override fun createState(stateId: String, store: WorkerStateStore<ExternalPaymentState>): ExternalPaymentState? = null

    override fun createDefaultState(): ExternalPaymentState =
        error("ExternalPaymentStateMachine: no row for this payment — cannot synthesize default state")
}

class ExternalPaymentStateMachine @AssistedInject constructor(
    @Assisted key: ExternalPaymentKey,
    sessionFactory: ExternalPaymentLocalSession.Factory,
) : BaseWorkerStateMachine<ExternalPaymentState, Unit>(
    localSession = sessionFactory.create(key),
    stateFactory = ExternalPaymentStateFactory(),
) {
    override suspend fun createTransition() = Unit

    @AssistedFactory
    interface Factory {
        fun create(key: ExternalPaymentKey): ExternalPaymentStateMachine
    }
}
