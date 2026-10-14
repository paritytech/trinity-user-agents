import Foundation

public enum ExternalPaymentError: Error, Equatable {
    /// A payment with the same `(productId, paymentId)` is already registered.
    case alreadyExists
    /// Nothing is registered under `(productId, paymentId)`.
    case notFound
    case invalidPaymentId
}
