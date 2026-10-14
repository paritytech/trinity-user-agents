import ExtrinsicService
import Foundation
import SDKLogger
import SubstrateSdk

/// Makes a built extrinsic a transaction's next attempt and puts it on the wire.
///
/// The executor's one route out to the chain, behind a protocol so what it does with a policy's output —
/// the buckets, the backoff, the cooldown — can be exercised without a connection.
public protocol DurableAttemptStarting: Sendable {
    /// Makes `model` the attempt of a transaction waiting to be built, then watches it. Returns whether
    /// the transaction was still waiting.
    func startAttempt(id: DurableTxId, model: ExtrinsicBuiltModel, chainId: ChainId) async throws -> Bool
}

/// Puts an attempt on the wire and follows it: the submission watch that registration starts, and the
/// same watch for an attempt a policy built later, so a rebuilt transaction lands as fast as a fresh one.
public struct DurableSubmissionLauncher: DurableAttemptStarting, Sendable {
    private let store: any DurableTxRepositoryProtocol
    private let tracker: DurableTxTracker
    private let owned: DurableTxOwnershipSet
    private let chainTools: any DurableChainToolsProviding
    private let onRecovery: @Sendable () -> Void
    private let logger: SDKLoggerProtocol?

    public init(
        store: any DurableTxRepositoryProtocol,
        tracker: DurableTxTracker,
        owned: DurableTxOwnershipSet,
        chainTools: any DurableChainToolsProviding,
        onRecovery: @escaping @Sendable () -> Void,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.tracker = tracker
        self.owned = owned
        self.chainTools = chainTools
        self.onRecovery = onRecovery
        self.logger = logger
    }

    /// Watches an attempt whose ownership the caller already took inside the registration transaction.
    public func watch(
        id: DurableTxId,
        model: ExtrinsicBuiltModel,
        chainId: ChainId,
        submitter: any ExtrinsicSubmitting
    ) {
        let submission = DurableTxTracker.Submission(
            model: model,
            transactionId: id,
            chainId: chainId,
            submitter: submitter
        )

        tracker.track(submission, onRecovery: onRecovery)
    }

    /// Makes `model` the attempt of a transaction waiting to be built, then watches it. Returns whether
    /// the transaction was still waiting.
    ///
    /// Ownership is taken before the row turns `pending`, so a pass can never evaluate the new attempt
    /// underneath its watch. Bytes identical to an attempt already released cannot be owned again, and so
    /// must not be started.
    public func startAttempt(
        id: DurableTxId,
        model: ExtrinsicBuiltModel,
        chainId: ChainId
    ) async throws -> Bool {
        let attempt = try DurableTxAttempt(from: model)

        guard owned.take(id, txHash: attempt.txHash) else {
            logger?.warning("Attempt already watched id=\(id) hash=\(attempt.txHash.toHex())")

            return false
        }

        do {
            let submitter = try await chainTools.extrinsicSubmitter(for: chainId)
            let started = try await store.startAttempt(id: id, attempt: attempt)

            if started {
                watch(id: id, model: model, chainId: chainId, submitter: submitter)
            } else {
                owned.release(id, txHash: attempt.txHash)
            }

            return started
        } catch {
            owned.release(id, txHash: attempt.txHash)

            throw error
        }
    }
}
