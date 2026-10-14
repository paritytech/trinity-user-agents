package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId

typealias PaymentId = String
typealias PaymentOrigin = String

/** Ids are chosen by the origin, so they are only unique within it. */
data class ExternalPaymentKey(
    val origin: PaymentOrigin,
    val id: PaymentId,
)

data class PaymentContext(
    val key: ExternalPaymentKey,
    val amount: Balance,
    val destination: AccountId,
)

sealed interface PaymentStatus {
    data object Processing : PaymentStatus
    data object Completed : PaymentStatus
    data class PartiallyClaimed(val claimed: Balance) : PaymentStatus
    data class Failed(val reason: String) : PaymentStatus
}

sealed class ExternalPaymentError(message: String) : RuntimeException(message) {
    class AlreadyExists(key: ExternalPaymentKey) : ExternalPaymentError("Payment $key already exists")

    class NotFound(key: ExternalPaymentKey) : ExternalPaymentError("Payment $key not found")
}
