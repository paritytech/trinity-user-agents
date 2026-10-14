import AsyncExtensions
import Foundation

/// Persistence interface for external payment records.
public protocol ExternalPaymentStoring: Sendable {
    func save(payment: ExternalPayment) async throws
    func fetchPayment(byId id: String) async throws -> ExternalPayment?
    func observePayment(id: String) -> AnyAsyncSequence<ExternalPayment?>

    /// Streams snapshots of non-terminal payments (plan, onboardCoins, offboardVouchers).
    /// Emits a new snapshot whenever any matching payment changes.
    func observeNonTerminalPayments() -> AnyAsyncSequence<[ExternalPayment]>
}
