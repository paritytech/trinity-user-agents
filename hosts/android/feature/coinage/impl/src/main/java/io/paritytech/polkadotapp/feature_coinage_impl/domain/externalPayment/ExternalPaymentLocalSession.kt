package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.data.worker.stateMachine.WorkerStateMachineLocalSession
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.database.dao.ExternalPaymentDao
import io.paritytech.polkadotapp.database.model.ExternalPaymentLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentContext
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model.ExternalPayment
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository.toDomainStage
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository.toRow
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.AwaitRecyclingPaymentState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.CompletedPaymentState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.EnsureVouchersPaymentState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.ExternalPaymentState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.FailedPaymentState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.OffboardVouchersPaymentState
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.state.PartiallyCompletedPaymentState
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

class ExternalPaymentLocalSession @AssistedInject constructor(
    @Assisted private val key: ExternalPaymentKey,
    private val dao: ExternalPaymentDao,
    private val selectedVoucherKeysCodec: SelectedVoucherKeysCodec,
    private val ensureVouchersFactory: EnsureVouchersPaymentState.Factory,
    private val awaitRecyclingFactory: AwaitRecyclingPaymentState.Factory,
    private val offboardVouchersFactory: OffboardVouchersPaymentState.Factory,
) : WorkerStateMachineLocalSession<ExternalPaymentState> {
    @AssistedFactory
    interface Factory {
        fun create(key: ExternalPaymentKey): ExternalPaymentLocalSession
    }

    override suspend fun getCurrentState(): ExternalPaymentState? =
        dao.getById(key.origin, key.id)?.toState()

    override suspend fun setCurrentState(state: ExternalPaymentState) {
        val row = state.toDomainStage().toRow(selectedVoucherKeysCodec)
        dao.updateStage(
            origin = key.origin,
            id = key.id,
            stage = row.stage,
            selectedVoucherKeys = row.selectedVoucherKeys,
            surplusPlanks = row.surplusPlanks,
            claimedPlanks = row.claimedPlanks,
            failureReason = row.failureReason,
            updatedAt = System.currentTimeMillis(),
        )
    }

    override fun currentStateFlow(): Flow<ExternalPaymentState?> =
        dao.observeById(key.origin, key.id).map { it?.toState() }

    override suspend fun resetState() {
        // Terminal rows are intentionally kept (retention policy).
    }

    private fun ExternalPaymentLocal.toState(): ExternalPaymentState {
        val context = PaymentContext(
            key = ExternalPaymentKey(origin = origin, id = id),
            amount = amountPlanks.intoBalance(),
            destination = destination.intoAccountId(),
        )
        return when (val stage = toDomainStage(selectedVoucherKeysCodec)) {
            ExternalPayment.Stage.EnsureVouchers -> ensureVouchersFactory.create(context)
            is ExternalPayment.Stage.AwaitRecycling -> awaitRecyclingFactory.create(context, stage.exactVoucherKeys)
            is ExternalPayment.Stage.OffboardVouchers -> offboardVouchersFactory.create(
                context = context,
                selected = stage.selectedVoucherKeys,
                surplusPlanks = stage.surplus.value,
            )
            ExternalPayment.Stage.Completed -> CompletedPaymentState(context)
            is ExternalPayment.Stage.PartiallyCompleted -> PartiallyCompletedPaymentState(context, stage.claimed)
            is ExternalPayment.Stage.Failed -> FailedPaymentState(context, stage.reason)
        }
    }

    private fun ExternalPaymentState.toDomainStage(): ExternalPayment.Stage = when (this) {
        is EnsureVouchersPaymentState -> ExternalPayment.Stage.EnsureVouchers
        is AwaitRecyclingPaymentState -> ExternalPayment.Stage.AwaitRecycling(exactVouchers)
        is OffboardVouchersPaymentState -> ExternalPayment.Stage.OffboardVouchers(selected, surplus)
        is CompletedPaymentState -> ExternalPayment.Stage.Completed
        is PartiallyCompletedPaymentState -> ExternalPayment.Stage.PartiallyCompleted(claimed)
        is FailedPaymentState -> ExternalPayment.Stage.Failed(reason)
    }
}
