package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import android.database.sqlite.SQLiteConstraintException
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.mapErrorInstance
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentError
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentService
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentWorkerStarter
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model.ExternalPayment
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository.ExternalPaymentRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformWhile
import javax.inject.Inject

class RealExternalPaymentService @Inject constructor(
    private val repository: ExternalPaymentRepository,
    private val workerStarter: ExternalPaymentWorkerStarter,
) : ExternalPaymentService {
    override suspend fun initiatePayment(
        key: ExternalPaymentKey,
        amount: Balance,
        destination: AccountId,
    ): Result<Unit> = runCancellableCatching {
        repository.insert(ExternalPayment.new(key = key, amount = amount, destination = destination))
    }
        .mapErrorInstance<_, SQLiteConstraintException> { error -> ExternalPaymentError.AlreadyExists(key) }
        .onSuccess { workerStarter.start() }

    override suspend fun exists(key: ExternalPaymentKey): Result<Boolean> = runCancellableCatching {
        repository.exists(key)
    }

    override fun subscribePaymentStatus(key: ExternalPaymentKey): Flow<PaymentStatus> = repository.observe(key)
        .map { payment -> payment?.toStatus() ?: throw ExternalPaymentError.NotFound(key) }
        .distinctUntilChanged()
        .transformWhile { status ->
            emit(status)
            status is PaymentStatus.Processing
        }

    private fun ExternalPayment.toStatus(): PaymentStatus = when (val s = stage) {
        ExternalPayment.Stage.Completed -> PaymentStatus.Completed
        is ExternalPayment.Stage.PartiallyCompleted -> PaymentStatus.PartiallyClaimed(s.claimed)
        is ExternalPayment.Stage.Failed -> PaymentStatus.Failed(s.reason)

        ExternalPayment.Stage.EnsureVouchers,
        is ExternalPayment.Stage.AwaitRecycling,
        is ExternalPayment.Stage.OffboardVouchers -> PaymentStatus.Processing
    }
}
