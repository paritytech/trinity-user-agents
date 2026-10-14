package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

/** RFC-0006 top-up failures, as the product sees them. */
sealed class TopUpError(message: String) : RuntimeException(message) {
    /** The source account was not found or is invalid. */
    class InvalidSource(cause: Throwable) : TopUpError("top-up source is invalid: ${cause.message}")

    /**
     * A top-up already exists under this id.
     *
     * Never a way to rejoin one: an id that is already taken says nothing about the amount or the source the
     * caller passed this time, and honouring the second of two conflicting requests under one id is how a
     * product ends up paying twice. Following it is what `paymentTopUpStatusSubscribe` is for.
     */
    class AlreadyExists(id: PaymentTopUpId) : TopUpError("a top-up already exists for id ${id.asHex()}")

    /**
     * Another top-up is already drawing on this source.
     *
     * One claim per source at a time: two running together race for the same money, and the one that loses
     * is a transaction the chain refuses. The source frees up when the top-up holding it reaches a verdict.
     */
    class SourceBusy(busyWith: PaymentTopUpId) :
        TopUpError("the source is already being claimed by top-up ${busyWith.asHex()}")

    /** No top-up was ever registered under this id. */
    class NotFound(id: PaymentTopUpId) : TopUpError("no top-up found for id ${id.asHex()}")
}
