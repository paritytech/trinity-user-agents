package io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest

/** Payment request failures, as the product sees them. */
sealed class PaymentRequestError(message: String) : RuntimeException(message) {
    /** Never a way to rejoin a payment: following one is what the status subscription is for. */
    class AlreadyExists(id: ProductPaymentRequestId) : PaymentRequestError("a payment already exists for id ${id.asHex()}")

    class Rejected : PaymentRequestError("payment rejected")

    class InsufficientBalance : PaymentRequestError("insufficient balance")

    class NotFound(id: ProductPaymentRequestId) : PaymentRequestError("no payment found for id ${id.asHex()}")
}
