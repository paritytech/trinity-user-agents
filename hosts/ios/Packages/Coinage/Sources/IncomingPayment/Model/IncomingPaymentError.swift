import Foundation

/// Errors surfaced by `IncomingPaymentServicing.accept`. Mirrors the host API `PaymentTopUpErr`.
public enum IncomingPaymentError: Error, Equatable {
    /// A payment with the same id already exists (idempotency key reuse).
    case alreadyExists
    /// The source material could not produce a valid keypair.
    case invalidSource(reason: String)
    /// The same source material is already used by another active payment.
    case sourceBusy
    /// A top-up of nothing: `amount` must be positive.
    case invalidAmount

    /// no operation found for given payment id
    case notFound(IncomingPaymentId)

    /// Any other failure. `reason` must be meaningful, never a bare type dump.
    case unknown(reason: String)
}
