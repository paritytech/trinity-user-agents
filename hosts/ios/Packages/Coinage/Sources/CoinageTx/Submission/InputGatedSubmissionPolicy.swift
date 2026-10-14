import DurableTransactions
import Foundation
import SDKLogger
import SubstrateSdk

/// Builds coinage transactions from what the ledger recorded for them, once the inputs they spend are
/// present on chain.
///
/// Every call resolves its transactions, waits for their inputs together, then builds the ones whose
/// inputs are all present — each rebuild consuming and minting exactly what was registered — and gives
/// up on the ones whose inputs are proven gone past their deadline. The rest keep waiting for a later
/// call. What differs between kinds of transaction is ``CoinageRebuild``'s.
final class InputGatedSubmissionPolicy<Rebuild: CoinageRebuild>: DurableSubmissionPolicy {
    let chainId: ChainId

    private let policyId: SubmissionPolicyId
    private let rebuild: Rebuild
    private let ledger: any CoinageAssetLedgerProtocol
    private let timing: InputWaitTiming
    private let logger: SDKLoggerProtocol?

    init(
        policyId: SubmissionPolicyId,
        chainId: ChainId,
        rebuild: Rebuild,
        ledger: any CoinageAssetLedgerProtocol,
        timing: InputWaitTiming = .production,
        logger: SDKLoggerProtocol?
    ) {
        self.policyId = policyId
        self.chainId = chainId
        self.rebuild = rebuild
        self.ledger = ledger
        self.timing = timing
        self.logger = logger
    }

    /// Whether a rebuild can still land depends on the chain, which only ``prepareSubmission(_:)`` may
    /// read. This weighs the recorded terms alone.
    func canRetry(_: DurableTxEntry, params: Data, failure: DurableFailureKind) async -> Bool {
        guard let terms = rebuild.terms(of: params) else { return false }

        let now = await timing.dateProvider.read()

        return terms.retriesFailures && retryableFailure(failure, now: now, deadline: terms.deadline)
    }

    func prepareSubmission(
        _ transactions: [ScheduledDurableTx]
    ) async throws -> [DurableTxId: SubmissionPreparation] {
        let resolution = try await resolve(transactions)

        return try await decide(resolution)
    }
}

// MARK: - Resolving

private extension InputGatedSubmissionPolicy {
    struct Waiting {
        let id: CoinageTxId
        let transaction: Rebuild.Transaction
        let inputs: Set<Rebuild.InputKey>
        let terms: RebuildTerms
    }

    struct Resolution {
        let waiting: [Waiting]
        let unbuildable: [CoinageTxId]
    }

    func resolve(_ transactions: [ScheduledDurableTx]) async throws -> Resolution {
        let assets = try await ledger.assets(of: transactions.map(\.id))
        let resolved = try await rebuild.resolve(transactions, assets: assets)

        let waiting = transactions.compactMap { transaction -> Waiting? in
            guard let resolvedTransaction = resolved[transaction.id],
                  let terms = rebuild.terms(of: transaction.policy.params)
            else {
                return nil
            }

            return Waiting(
                id: transaction.id,
                transaction: resolvedTransaction,
                inputs: rebuild.inputs(of: resolvedTransaction),
                terms: terms
            )
        }

        let buildable = Set(waiting.map(\.id))
        let unbuildable = transactions.map(\.id).filter { !buildable.contains($0) }

        for id in unbuildable {
            // Genuinely unbuildable, not merely unread: an unreadable store now throws out of
            // `resolve` and is retried, so anything reaching here is a row nothing will ever fix.
            logger?.error("\(policyId) rebuild impossible entry=\(id) reason=params-or-outputs-unresolvable")
        }

        return Resolution(waiting: waiting, unbuildable: unbuildable)
    }
}

// MARK: - Deciding

private extension InputGatedSubmissionPolicy {
    func decide(_ resolution: Resolution) async throws -> [DurableTxId: SubmissionPreparation] {
        guard !resolution.waiting.isEmpty else {
            return givenUp(resolution.unbuildable)
        }

        let look = try await awaitInputs(of: resolution.waiting)
        let ready = resolution.waiting.filter { $0.inputs.isSubset(of: look.present) }
        let notReady = resolution.waiting.filter { !$0.inputs.isSubset(of: look.present) }

        let built = try await build(ready)

        return built
            .merging(givenUp(abandoned(notReady, look: look))) { current, _ in current }
            .merging(givenUp(resolution.unbuildable)) { current, _ in current }
    }

    func awaitInputs(of waiting: [Waiting]) async throws -> InputsLook<Rebuild.InputKey> {
        let inputs = waiting.reduce(into: Set<Rebuild.InputKey>()) { $0.formUnion($1.inputs) }

        // The earliest deadline in the call decides when a look is the last word: a transaction that
        // may still give up must not be held open by one that will keep waiting. With none to compare,
        // now is already the deadline.
        let deadline: Date =
            if let earliest = waiting.map(\.terms.deadline).min() {
                earliest
            } else {
                await timing.dateProvider.read()
            }

        return try await Coinage.awaitInputs(
            presence: rebuild.presence(of: inputs),
            wanted: inputs,
            deadline: deadline,
            timing: timing
        )
    }

    /// Only an input proven absent past its own deadline ends a transaction's rebuilds.
    func abandoned(_ notReady: [Waiting], look: InputsLook<Rebuild.InputKey>) -> [CoinageTxId] {
        notReady
            .filter { waiting in
                waiting.inputs.contains { look.abandoned($0, deadline: waiting.terms.deadline) }
            }
            .map { waiting in
                logger?.info("\(policyId) rebuild abandoned entry=\(waiting.id) until=\(waiting.terms.deadline)")

                return waiting.id
            }
    }

    func build(_ ready: [Waiting]) async throws -> [DurableTxId: SubmissionPreparation] {
        guard !ready.isEmpty else { return [:] }

        let models = try await rebuild.build(ready.map(\.transaction))

        return zip(ready, models).reduce(into: [:]) { result, pair in
            result[pair.0.id] = SubmissionPreparation.ready(pair.1)
        }
    }

    func givenUp(_ ids: [CoinageTxId]) -> [DurableTxId: SubmissionPreparation] {
        ids.reduce(into: [:]) { $0[$1] = SubmissionPreparation.giveUp }
    }
}
