package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.last

interface ExternalPaymentService {
    /**
     * Registers the payment and hands it to the background worker. Returns once it is registered, not once it
     * is paid. Fails with [ExternalPaymentError.AlreadyExists] when [key] is already taken.
     */
    suspend fun initiatePayment(
        key: ExternalPaymentKey,
        amount: Balance,
        destination: AccountId,
    ): Result<Unit>

    suspend fun exists(key: ExternalPaymentKey): Result<Boolean>

    /**
     * Completes after the first terminal status. Fails with [ExternalPaymentError.NotFound] when nothing is
     * registered under [key].
     */
    fun subscribePaymentStatus(key: ExternalPaymentKey): Flow<PaymentStatus>
}

suspend fun ExternalPaymentService.awaitTransferOutcome(key: ExternalPaymentKey): Result<Unit> = runCancellableCatching {
    when (val terminalStatus = subscribePaymentStatus(key).last()) {
        is PaymentStatus.Completed, is PaymentStatus.PartiallyClaimed -> Unit
        is PaymentStatus.Failed -> throw ExternalPaymentFailedException(terminalStatus.reason)
        is PaymentStatus.Processing -> error("Payment did not reach terminal state")
    }
}

class ExternalPaymentFailedException(reason: String) : RuntimeException(reason)
