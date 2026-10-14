import Foundation

/// The durability layer's entry points, handed to a domain as one value.
///
/// A domain needs four distinct things and they have different lifetimes: it *registers* its oracle
/// and its policies once at setup, *builds* extrinsics whenever it has bytes to produce, and *submits*
/// for the life of the process. Keeping them apart is what stops the submission API from growing
/// setup-only members that every test double then has to implement.
public struct DurableTxServices: Sendable {
    /// Submits, schedules and answers status — what a domain uses after setup.
    public let txService: any DurableTxServicing

    /// Where a domain registers its ``TxCompletionOracle`` before it submits anything.
    public let oracles: TxCompletionOracleRegistry

    /// Where a domain registers its ``DurableSubmissionPolicy`` before it schedules anything
    /// carrying one.
    public let policies: DurableSubmissionPolicyRegistry

    /// Builds declared transactions into signed extrinsics.
    public let factory: any DurableTxMaking

    public init(
        txService: any DurableTxServicing,
        oracles: TxCompletionOracleRegistry,
        policies: DurableSubmissionPolicyRegistry,
        factory: any DurableTxMaking
    ) {
        self.txService = txService
        self.oracles = oracles
        self.policies = policies
        self.factory = factory
    }
}
