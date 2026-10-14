import Foundation
import Operation_iOS
import SubstrateSdk

/// A durable, restart-recoverable top-up. The record holds no secret material — the source lives in
/// the encrypted `IncomingPaymentSecretStoring` — and no live status. It persists only what cannot be
/// recomputed: the product-supplied id, the product it is bound to, the amount, when its retry window
/// opened, and — once there is one — its terminal ``outcome`` (read back exactly, never re-derived).
public struct IncomingPayment: Equatable, Sendable {
    /// The product-supplied idempotency key. Unique only within a product.
    public let paymentId: IncomingPaymentId
    /// The product this payment is bound to. `(productId, paymentId)` is the global identity.
    public let productId: String
    public let amount: Balance
    /// The retry window opened here; a resumed run finishes the window it was given.
    public let createdAt: Date
    /// The immutable verdict, `nil` until nothing further will be attempted. Its presence is the
    /// "inactive/complete" signal — `setup()` never starts a task for a settled record.
    public let outcome: IncomingPaymentTerminalOutcome?

    public init(
        paymentId: IncomingPaymentId,
        productId: String,
        amount: Balance,
        createdAt: Date,
        outcome: IncomingPaymentTerminalOutcome?
    ) {
        self.paymentId = paymentId
        self.productId = productId
        self.amount = amount
        self.createdAt = createdAt
        self.outcome = outcome
    }
}

public extension IncomingPayment {
    /// The CoinageTx durability group the claim registers under — product-bound and `"top up:"`
    /// prefixed, so the same `paymentId` from different products never collides and a top-up group
    /// never clashes with a transfer group (`groupId = messageId`).
    var groupId: CoinageTxGroupId { Self.groupId(productId: productId, paymentId: paymentId) }

    /// A payment is active until it is settled with a terminal ``outcome``.
    var isActive: Bool { outcome == nil }

    /// The durability group / record identity for a `(productId, paymentId)` pair.
    static func groupId(productId: String, paymentId: IncomingPaymentId) -> CoinageTxGroupId {
        "top up:\(productId):\(paymentId)"
    }
}

extension IncomingPayment: Operation_iOS.Identifiable {
    /// The persistence identity — the product-bound `groupId`.
    public var identifier: String { groupId }
}
