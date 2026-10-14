import AsyncExtensions
import Foundation
import SubstrateSdk

/// Idempotent, restart-durable inbound top-ups. Instantiated and driven by the ServiceCoordinator.
public protocol IncomingPaymentServicing: Sendable {
    /// Registers a top-up for `(productId, paymentId)` from `descriptor` and returns once
    /// initialization has concluded — the claim is driven by `setup`'s subscription. Throws
    /// `IncomingPaymentError`.
    func accept(
        amount: Balance,
        descriptor: IncomingPaymentSourceDescriptor,
        paymentId: IncomingPaymentId,
        productId: String
    ) async throws

    /// Observes a payment's status, scoped to `(productId, paymentId)`. Returns the live derived
    /// stream for an active payment, or the stored terminal verdict for a settled one.
    func subscribeStatus(
        for paymentId: IncomingPaymentId,
        productId: String
    ) async throws -> AnyAsyncSequence<IncomingPaymentStatus>

    /// Subscribes to active payments and (re)starts their claim tasks, using the resolved denomination
    /// context. Idempotent across restarts. Driven by `CoinageService.setup(with:)` once the context is available,
    /// mirroring
    /// `ExternalPaymentServicing`.
    func setup(with denomination: DenominationBreakdownContext)
}
