import DurableTransactions
import DurableTransactionsTestSupport
import ExtrinsicService
import Foundation
import Testing
@testable import Coinage

/// Driving the harness's transactions through a *policy* rather than a finished extrinsic: scheduling
/// one with locked assets but no bytes, letting the executor build it, and failing an attempt the way
/// the chain would.
///
/// The attempt starter stands in for the launcher — it does to the ledger exactly what the launcher does
/// (`startAttempt`) and skips only the chain watch, which the scenarios here are not about.
extension DurabilityHarness {
    /// Registers `policy` under `id`, so rows naming it can be built and their failures weighed.
    func installPolicy(_ policy: any DurableSubmissionPolicy, as id: SubmissionPolicyId) {
        policies.register(policy, for: id)
    }

    /// Schedules a transaction with its assets locked and no attempt — what a domain registers when the
    /// policy, not the caller, owns building it.
    @discardableResult
    func schedule(
        inputs: [CoinageTxInput],
        outputs: [OwnAsset],
        policyId: SubmissionPolicyId,
        params: Data = Data(),
        groupId: CoinageTxGroupId? = nil
    ) async throws -> CoinageTxId {
        let ledger = store.ledger
        let assets = CoinageAssetRegistration(inputs: inputs, outputs: outputs)

        let ids = try await store.durable.schedule(
            [DurableTxSchedule(
                domainId: .coinage,
                groupId: groupId,
                policy: SubmissionPolicy(id: policyId, params: params)
            )],
            in: nil,
            onRegister: { scope, ids in
                try ledger.registerAssets([assets], for: ids, in: scope)
            }
        )

        return try #require(ids.first)
    }

    /// Schedules a claim: a coin a peer handed us, minted into one of ours.
    @discardableResult
    func scheduleClaim(
        receivedCoin: CoinageKeyIndex,
        outputCoin: CoinageKeyIndex,
        policyId: SubmissionPolicyId,
        params: Data = Data(),
        groupId: CoinageTxGroupId? = nil
    ) async throws -> CoinageTxId {
        try await schedule(
            inputs: [.coin(.received(HarnessKeys.coinKey(receivedCoin)))],
            outputs: [coinOutput(outputCoin)],
            policyId: policyId,
            params: params,
            groupId: groupId
        )
    }

    /// Runs the executor until `condition` holds, then stops it.
    ///
    /// A condition that never holds times out and the scenario's own expectation reports what was
    /// actually reached, so a stuck executor fails as a wrong state rather than as a hang.
    func runExecutor(
        starter: ScriptedAttemptStarter,
        until condition: @escaping @Sendable () async throws -> Bool
    ) async {
        let executor = DurableSubmissionExecutor(
            store: store.durable,
            policies: policies,
            launcher: starter,
            onPendingSubmissions: {},
            backgroundExecutor: backgroundExecutor,
            timing: .harness,
            logger: nil
        )

        await executor.ensureStarted()

        // A backstop against a stalled executor, not an expected duration: the loop exits as soon as the
        // condition holds. Generous because a loaded CI runner can stall for seconds, and a budget that
        // expires early would fail a scenario that is merely slow.
        let deadline = ContinuousClock.now.advanced(by: .seconds(60))
        while ContinuousClock.now < deadline {
            if await (try? condition()) == true { break }

            try? await Task.sleep(for: .milliseconds(1))
        }

        await executor.close()
    }

    /// An attempt starter that writes through to this harness's ledger, as the launcher does.
    func makeAttemptStarter() -> ScriptedAttemptStarter {
        ScriptedAttemptStarter(writingTo: store.durable)
    }

    /// Reports the verdict the chain gave an attempt, through the same writer the pass and the
    /// submission watch use — so whether it becomes a retry is decided by the policy, here as in
    /// production.
    @discardableResult
    func reportFailure(_ id: CoinageTxId, kind: DurableFailureKind) async throws -> Bool {
        let entry = try #require(try await store.durable.getEntry(id: id))

        return try await verdictWriter.write(
            entry,
            Verdict(status: .failure, successDetectedAt: nil, failure: kind)
        )
    }

    /// The assets a row still holds, which must survive every rebuild: a payment already made out of a
    /// claim's output coin is waiting on that exact coin.
    func assets(of id: CoinageTxId) async throws -> (inputs: [CoinageTxInput], outputs: [OwnAsset]) {
        let entry = try #require(try await store.ledger.getEntry(id: id))

        return (entry.inputs, entry.outputs)
    }

    func attemptHash(of id: CoinageTxId) async throws -> Data? {
        try await store.durable.getEntry(id: id)?.attempt?.txHash
    }
}

extension DurableSubmissionExecutor.Timing {
    /// Real waits, small enough that a rebuild loop runs in milliseconds and still yields between rounds.
    static let harness = DurableSubmissionExecutor.Timing(
        initialBackoff: .milliseconds(1),
        maxBackoff: .milliseconds(1),
        rebuildCooldown: .milliseconds(1),
        maxRebuildCooldown: .milliseconds(1),
        maxDoublings: 0,
        clock: ContinuousClock()
    )
}
