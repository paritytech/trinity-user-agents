import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import Coinage

/// A transaction the policy owns, across its whole life: scheduled with its assets locked and no bytes,
/// built, failed by the chain, and built again.
///
/// The invariant every one of these turns on is that a rebuild keeps the *row* — same id, same group,
/// same inputs and outputs — and replaces only the attempt. That is what lets a payment already made out
/// of a claim's output coin keep waiting on it: a claim retried into a fresh coin would leave that
/// payment waiting for a coin that will never exist.
@Suite("Rebuild Scenarios")
struct RebuildScenariosTest {
    private let receivedCoin: CoinageKeyIndex = 1
    private let mintedCoin: CoinageKeyIndex = 2
    private let policyId = SubmissionPolicyId("harness-claim")

    @Test("a scheduled transaction locks its assets before it has any bytes")
    func schedulingLocksAssetsWithoutAnAttempt() async throws {
        let harness = DurabilityHarness()
        let policy = ScriptedSubmissionPolicy(chainId: harnessChainId)
        harness.installPolicy(policy, as: policyId)

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId
        )

        #expect(try await harness.status(of: id) == .pendingSubmission)
        #expect(try await harness.attemptHash(of: id) == nil)
        // The output coin is already the row's, so nothing else can mint or select it.
        #expect(try await harness.assets(of: id).outputs == [harness.coinOutput(mintedCoin)])
    }

    @Test("the policy builds the scheduled transaction into its first attempt")
    func policyBuildsTheFirstAttempt() async throws {
        let harness = DurabilityHarness()
        let policy = ScriptedSubmissionPolicy(chainId: harnessChainId)
        policy.script(.buildAll)
        harness.installPolicy(policy, as: policyId)
        let starter = harness.makeAttemptStarter()

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId
        )
        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .pending
        }

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.attemptHash(of: id) != nil)
    }

    @Test("a reorged claim is rebuilt into the same output coin")
    func reorgedClaimKeepsItsOutputCoin() async throws {
        let harness = DurabilityHarness()
        let policy = ScriptedSubmissionPolicy(chainId: harnessChainId)
        policy.script(.buildAll)
        policy.answerRetry(true)
        harness.installPolicy(policy, as: policyId)
        let starter = harness.makeAttemptStarter()

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId
        )
        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .pending
        }
        let firstAttempt = try #require(try await harness.attemptHash(of: id))

        // The attempt never made it into the canonical chain and its window has closed.
        #expect(try await harness.reportFailure(id, kind: .expired))
        #expect(try await harness.status(of: id) == .pendingSubmission)

        // The policy builds it again — into different bytes, but the same row.
        policy.script(.perId { transactions in
            transactions.reduce(into: [:]) { result, transaction in
                result[transaction.id] = .ready(ScriptedSubmissionPolicy.model(for: transaction.id, attempt: 1))
            }
        })
        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .pending
        }

        let secondAttempt = try #require(try await harness.attemptHash(of: id))
        #expect(secondAttempt != firstAttempt)
        // The whole point: the coin the peer's payment is waiting on is still this row's output.
        let assets = try await harness.assets(of: id)
        #expect(assets.outputs == [harness.coinOutput(mintedCoin)])
        #expect(assets.inputs == [.coin(.received(HarnessKeys.coinKey(receivedCoin)))])
    }

    @Test("a policy that declines leaves the failure terminal")
    func declinedRetryStaysFailed() async throws {
        let harness = DurabilityHarness()
        let policy = ScriptedSubmissionPolicy(chainId: harnessChainId)
        policy.script(.buildAll)
        policy.answerRetry(false)
        harness.installPolicy(policy, as: policyId)
        let starter = harness.makeAttemptStarter()

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId
        )
        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .pending
        }

        #expect(try await harness.reportFailure(id, kind: .dispatchFailed))
        #expect(try await harness.status(of: id) == .failure)
    }

    @Test("a rebuilt transaction keeps its group, so the group is never read as finished")
    func rebuildKeepsItsGroup() async throws {
        let harness = DurabilityHarness()
        let policy = ScriptedSubmissionPolicy(chainId: harnessChainId)
        policy.script(.buildAll)
        policy.answerRetry(true)
        harness.installPolicy(policy, as: policyId)
        let starter = harness.makeAttemptStarter()
        let groupId = "message-1"

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId,
            groupId: groupId
        )

        // Even before it has bytes, the group must show the transaction — an empty group reads as done.
        #expect(try await harness.store.getOperationGroupStatuses(groupId).map(\.id) == [id])

        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .pending
        }
        #expect(try await harness.reportFailure(id, kind: .expired))

        #expect(try await harness.status(of: id) == .pendingSubmission)
        #expect(try await harness.store.getOperationGroupStatuses(groupId).map(\.id) == [id])
    }

    @Test("a transaction waiting to be built survives a crash and is built after it")
    func scheduledRowSurvivesACrash() async throws {
        let harness = DurabilityHarness()
        let policy = ScriptedSubmissionPolicy(chainId: harnessChainId)
        policy.script(.buildAll)
        harness.installPolicy(policy, as: policyId)

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId
        )

        harness.crash()

        #expect(try await harness.status(of: id) == .pendingSubmission)
        let starter = harness.makeAttemptStarter()
        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .pending
        }

        #expect(try await harness.status(of: id) == .pending)
        #expect(try await harness.assets(of: id).outputs == [harness.coinOutput(mintedCoin)])
    }

    @Test("a row whose policy is not registered is failed rather than left waiting")
    func unregisteredPolicyFailsTheRow() async throws {
        let harness = DurabilityHarness()
        // Nothing installed for this id.
        let starter = harness.makeAttemptStarter()

        let id = try await harness.scheduleClaim(
            receivedCoin: receivedCoin,
            outputCoin: mintedCoin,
            policyId: policyId
        )
        await harness.runExecutor(starter: starter) {
            try await harness.status(of: id) == .failure
        }

        #expect(try await harness.status(of: id) == .failure)
    }
}
