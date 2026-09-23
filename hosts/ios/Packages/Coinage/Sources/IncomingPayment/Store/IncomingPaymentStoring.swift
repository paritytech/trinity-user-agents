import AsyncExtensions
import Foundation

/// Persistence interface for incoming-payment records. Implemented in the main app target to bridge
/// CoreData to the package (mirrors `ExternalPaymentStoring`).
///
/// The record holds no secret material (that lives in `IncomingPaymentSecretStoring`) and no live
/// status. The only mutation is settlement, which writes the immutable terminal verdict once;
/// "active" means `outcome == nil`.
public protocol IncomingPaymentStoring: Sendable {
    /// Persists a new payment. Runs `authorization` inside the write transaction, immediately before
    /// mutation: a request that waited across account replacement must never create a resumable row.
    func save(
        _ payment: IncomingPayment,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws

    /// The payment with this `groupId` (`"top up:productId:paymentId"`), if any — the idempotency
    /// check for `accept`. Keying by `groupId` scopes the lookup to the product.
    func fetch(groupId: CoinageTxGroupId) async throws -> IncomingPayment?

    /// Every active (`outcome == nil`) payment — the busy-detection set and the resume set.
    func fetchActivePayments() async throws -> [IncomingPayment]

    /// Writes the terminal verdict once. Runs `authorization` inside the write transaction before
    /// mutation, and rejects an existing row whose owner differs from `ownerId`.
    func settle(
        groupId: CoinageTxGroupId,
        ownerId: Data,
        outcome: IncomingPaymentTerminalOutcome,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws

    /// Streams snapshots of the active (`outcome == nil`) payments — the stream `setup()` subscribes
    /// to for resume.
    func observeActivePayments() -> AnyAsyncSequence<[IncomingPayment]>
}
