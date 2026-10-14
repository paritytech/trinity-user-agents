import Foundation
import SDKLogger

/// Writes a verdict about one submitted attempt, for both the recovery pass and the submission watch.
///
/// A failure is where the two would otherwise diverge: a transaction whose policy still wants it goes
/// back to ``DurableTxStatus/pendingSubmission`` instead, keeping whatever its domain locked. Doing that
/// here, rather than in each writer, is what keeps a failure from ever becoming terminal through the
/// path that forgot.
public struct DurableVerdictWriter: Sendable {
    private let store: any DurableTxRepositoryProtocol
    private let policies: DurableSubmissionPolicyRegistry
    private let logger: SDKLoggerProtocol?

    public init(
        store: any DurableTxRepositoryProtocol,
        policies: DurableSubmissionPolicyRegistry,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.policies = policies
        self.logger = logger
    }

    /// Writes only while `observed` is still the transaction's status and attempt. Returns whether it
    /// wrote.
    @discardableResult
    public func write(_ observed: DurableTxEntry, _ verdict: Verdict) async throws -> Bool {
        // Only an attempt can be compared against, so a row without one is not this writer's to
        // decide — the executor owns it until it has bytes.
        guard let attempt = observed.attempt else { return false }

        let effective = try await effectiveVerdict(for: observed, verdict)

        return try await store.updateTxStatus(
            for: observed.id,
            expectedCurrentStatus: observed.status,
            expectedTxHash: attempt.txHash,
            verdict: effective
        )
    }
}

private extension DurableVerdictWriter {
    func effectiveVerdict(for observed: DurableTxEntry, _ verdict: Verdict) async throws -> Verdict {
        guard verdict.status == .failure, let failure = verdict.failure else {
            return verdict
        }

        return try await retryInstead(observed, failure: failure) ?? verdict
    }

    /// A policy that cannot be *read* throws rather than failing the transaction: the verdict is
    /// re-derived on the next pass, while a failure written now could never be taken back.
    func retryInstead(_ entry: DurableTxEntry, failure: DurableFailureKind) async throws -> Verdict? {
        guard let reference = try await store.getSubmissionPolicy(id: entry.id) else {
            return nil
        }

        guard let policy = policies.policy(for: reference.id) else {
            logger?.error("Retry skipped id=\(entry.id) reason=no-registered-policy policy=\(reference.id)")

            return nil
        }

        guard await policy.canRetry(entry, params: reference.params, failure: failure) else {
            logger?.debug("Retry declined id=\(entry.id) policy=\(reference.id) failure=\(failure)")

            return nil
        }

        logger?.info("Failure deferred to policy id=\(entry.id) policy=\(reference.id) failure=\(failure)")

        return Verdict(status: .pendingSubmission, successDetectedAt: nil, failure: nil)
    }
}
