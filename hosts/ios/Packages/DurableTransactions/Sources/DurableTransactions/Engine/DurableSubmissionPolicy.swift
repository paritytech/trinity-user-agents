@preconcurrency import ExtrinsicService
import Foundation
import SubstrateSdk

/// Builds a durable transaction outside the call that registered it: for the first time when it was
/// scheduled, and again when an attempt is proven unable to land.
///
/// A rebuild must consume and mint exactly what the domain registered for the transaction. The domain's
/// rows stay attached to the same id across attempts, and every lock and completion rule reads them —
/// which is what lets a payment made out of a reorged claim still land once the claim is rebuilt.
public protocol DurableSubmissionPolicy: Sendable {
    /// The chain the built extrinsics are submitted to.
    var chainId: ChainId { get }

    /// Whether `entry`, whose attempt was proven unable to land because of `failure`, should be built
    /// again instead of failing.
    ///
    /// Asked while a verdict is being written, so it must not read the chain. Whether a rebuild is still
    /// possible belongs to ``prepareSubmission(_:)``, which may take as long as it needs to find out. A
    /// failure that would repeat on the same effects must not be retried indefinitely: nothing else
    /// bounds the loop.
    func canRetry(_ entry: DurableTxEntry, params: Data, failure: DurableFailureKind) async -> Bool

    /// Builds whichever of `transactions` it can. Every one of them shares a policy and a group, so work
    /// common to them — pinned blocks, proofs, anything handed out per extrinsic — is done once per call.
    ///
    /// May suspend for as long as it waits for something on chain. A transaction absent from the result
    /// stays waiting and comes back in a later call; ``SubmissionPreparation/giveUp`` fails it for good.
    /// A thrown error is never a verdict: the call is simply made again after a backoff.
    func prepareSubmission(
        _ transactions: [ScheduledDurableTx]
    ) async throws -> [DurableTxId: SubmissionPreparation]
}

/// What a policy decided about one transaction it was asked to build.
public enum SubmissionPreparation: Sendable {
    /// Built and signed, ready to become this transaction's next attempt.
    case ready(ExtrinsicBuiltModel)

    /// Nothing further can make this transaction land.
    case giveUp
}
