import DurableTransactionsTestSupport
import Foundation
import Testing
@testable import DurableTransactions

/// What the executor does with a policy's answers: which transactions reach it together, what happens to
/// a row nothing can build, and that a policy which decides nothing is retried rather than abandoned.
///
/// Every backoff and cooldown runs on a test clock, so a suite that exercises the retry loop still
/// finishes in milliseconds.
@Suite("Submission Executor")
struct SubmissionExecutorTests {
    private let policyId = SubmissionPolicyId("test-policy")
    private let otherPolicyId = SubmissionPolicyId("other-policy")

    @Test("a scheduled transaction is built and started")
    func schedulingStartsAnAttempt() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .pending }

        #expect(harness.starter.starts.map(\.id) == [id])
        #expect(try await harness.store.getEntry(id: id)?.status == .pending)
    }

    @Test("the started extrinsic is the one the policy built")
    func startedBytesComeFromThePolicy() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .pending }

        let start = try #require(harness.starter.starts.first)
        #expect(start.extrinsic == ScriptedSubmissionPolicy.model(for: id).extrinsic)
        #expect(start.chainId == harness.policy.chainId)
    }

    @Test("transactions of one group reach the policy in a single call")
    func groupIsPreparedTogether() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        for _ in 0 ..< 3 {
            _ = try await harness.store.scheduleOne(policyId: policyId, groupId: "group-1")
        }

        await harness.run { harness.starter.starts.count == 3 }

        let firstCall = try #require(harness.policy.prepareCalls.first)
        #expect(firstCall.count == 3)
        #expect(harness.starter.starts.count == 3)
    }

    @Test("different groups are prepared separately")
    func groupsAreSeparateBuckets() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        _ = try await harness.store.scheduleOne(policyId: policyId, groupId: "group-1")
        _ = try await harness.store.scheduleOne(policyId: policyId, groupId: "group-2")

        await harness.run { harness.starter.starts.count == 2 }

        // One call each, never one call holding both groups.
        #expect(harness.policy.prepareCalls.allSatisfy { $0.count == 1 })
        #expect(harness.starter.starts.count == 2)
    }

    @Test("a policy that gives up fails the transaction for good")
    func giveUpAbandonsTheRow() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.giveUpAll)
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .failure }

        #expect(try await harness.store.getEntry(id: id)?.status == .failure)
        #expect(harness.starter.starts.isEmpty)
    }

    @Test("a row whose policy is not registered is abandoned rather than left waiting")
    func unregisteredPolicyAbandonsTheRow() async throws {
        let harness = Harness(policyId: policyId)
        // Scheduled under a policy id nothing registered.
        let id = try await harness.store.scheduleOne(policyId: otherPolicyId)

        await harness.run { try await harness.status(id) == .failure }

        #expect(try await harness.store.getEntry(id: id)?.status == .failure)
    }

    @Test("every row of an unregistered policy is failed, across groups")
    func unregisteredPolicyAbandonsEveryGroup() async throws {
        let harness = Harness(policyId: policyId)
        let first = try await harness.store.scheduleOne(policyId: otherPolicyId, groupId: "group-1")
        let second = try await harness.store.scheduleOne(policyId: otherPolicyId, groupId: "group-2")

        await harness.run {
            let firstStatus = try await harness.status(first)
            let secondStatus = try await harness.status(second)

            return firstStatus == .failure && secondStatus == .failure
        }

        #expect(try await harness.store.getEntry(id: first)?.status == .failure)
        #expect(try await harness.store.getEntry(id: second)?.status == .failure)
    }

    @Test("a policy that decides nothing keeps the transaction waiting and is asked again")
    func undecidedIsRetried() async throws {
        let harness = Harness(policyId: policyId)
        // Two rounds of waiting, then it builds — the ordinary shape of waiting for an input to appear.
        harness.policy.script(.waitAll, .waitAll, .buildAll)
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .pending }

        #expect(harness.policy.prepareCalls.count >= 3)
        #expect(try await harness.store.getEntry(id: id)?.status == .pending)
    }

    @Test("a throwing policy is never a verdict — the transaction is still waiting and retried")
    func thrownErrorIsNotAVerdict() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.fail, .fail, .buildAll)
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .pending }

        #expect(harness.policy.prepareCalls.count >= 3)
        #expect(try await harness.store.getEntry(id: id)?.status == .pending)
    }

    @Test("an extrinsic the engine refuses abandons the row instead of rebuilding for ever")
    func rejectedAttemptAbandonsTheRow() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        harness.starter.script(.reject(.notMortal))
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .failure }

        #expect(try await harness.store.getEntry(id: id)?.status == .failure)
    }

    @Test("a transient start failure leaves the transaction waiting")
    func transientStartFailureKeepsItWaiting() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        harness.starter.script(.fail(ScriptedPolicyError.scripted), .started)
        let id = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { try await harness.status(id) == .pending }

        #expect(harness.starter.starts.count >= 2)
        #expect(try await harness.store.getEntry(id: id)?.status == .pending)
    }

    @Test("a policy answering about a transaction it was not asked about is ignored")
    func outcomesForUnaskedTransactionsAreIgnored() async throws {
        let harness = Harness(policyId: policyId)
        let stranger = DurableTxId()
        harness.policy.script(.perId { _ in [stranger: .ready(ScriptedSubmissionPolicy.model(for: stranger))] })
        _ = try await harness.store.scheduleOne(policyId: policyId)

        await harness.run { harness.policy.prepareCalls.count >= 2 }

        #expect(harness.starter.starts.isEmpty)
    }

    @Test("only the policy's own transactions are prepared")
    func policySeesOnlyItsOwnRows() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        let mine = try await harness.store.scheduleOne(policyId: policyId)
        _ = try await harness.store.scheduleOne(policyId: otherPolicyId)

        await harness.run { try await harness.status(mine) == .pending }

        #expect(harness.policy.prepareCalls.allSatisfy { $0.allSatisfy { $0.id == mine } })
    }

    @Test("a collector whose stream ended is started again")
    func endedCollectorIsRestarted() async throws {
        // A stream that fails or completes returns the collector normally, and a task that returned is
        // not cancelled. Keying the restart on cancellation left the executor permanently deaf, so one
        // transient subscription failure stranded every scheduled transaction until relaunch.
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)

        await harness.executor.ensureStarted()
        // The collector subscribes inside its task, so wait for the stream to exist before ending it —
        // otherwise there is nothing to end and the test proves nothing.
        await harness.settle(until: { harness.store.pendingStreamCount > 0 })
        harness.store.finishPendingStreams()

        let id = try await harness.store.scheduleOne(policyId: policyId)
        await harness.run { try await harness.status(id) == .pending }

        #expect(try await harness.status(id) == .pending)
    }

    @Test("the policy is given the params the row was scheduled with")
    func scheduledParamsReachThePolicy() async throws {
        let harness = Harness(policyId: policyId)
        harness.policy.script(.buildAll)
        let params = Data([1, 2, 3, 4])
        _ = try await harness.store.scheduleOne(policyId: policyId, params: params)

        await harness.run { harness.policy.prepareCalls.count >= 1 }

        let asked = try #require(harness.policy.prepareCalls.first?.first)
        #expect(asked.policy.params == params)
        #expect(asked.policy.id == policyId)
    }
}

// MARK: - Harness

private extension SubmissionExecutorTests {
    /// The executor over an in-memory ledger, a scripted policy and a scripted starter, on a clock whose
    /// waits are a millisecond rather than minutes.
    final class Harness {
        let store = InMemoryDurableTxRepository()
        let policy = ScriptedSubmissionPolicy()
        let starter: ScriptedAttemptStarter
        let executor: DurableSubmissionExecutor

        init(policyId: SubmissionPolicyId) {
            let registry = DurableSubmissionPolicyRegistry()
            registry.register(policy, for: policyId)
            starter = ScriptedAttemptStarter(writingTo: store)

            executor = DurableSubmissionExecutor(
                store: store,
                policies: registry,
                launcher: starter,
                onPendingSubmissions: {},
                backgroundExecutor: StubBackgroundExecutor(),
                timing: .brisk,
                logger: nil
            )
        }

        /// Waits for `condition`, yielding between checks — for the points where a test has to know the
        /// executor's own task reached a state before it acts.
        func settle(until condition: @escaping @Sendable () -> Bool) async {
            let deadline = ContinuousClock.now.advanced(by: .seconds(60))
            while ContinuousClock.now < deadline {
                if condition() { return }

                try? await Task.sleep(for: .milliseconds(1))
            }
        }

        /// Starts the executor and waits for `condition`, then closes it.
        ///
        /// The executor is driven by the ledger rather than by the caller, so a test cannot know how many
        /// rounds its scenario takes: it names the state it is waiting for instead. A condition that never
        /// holds simply times out and the test's own expectation reports what was actually reached.
        func run(until condition: @escaping @Sendable () async throws -> Bool) async {
            // A backstop against a stalled executor, not an expected duration: the loop exits as soon as
            // the condition holds, so a passing test finishes in milliseconds. It is generous because a
            // loaded CI runner can stall for seconds, and a budget that expires early would fail a test
            // that is merely slow.
            let deadline = ContinuousClock.now.advanced(by: .seconds(60))
            while ContinuousClock.now < deadline {
                // Asked each round, as production does — the app asks on every new head and the call is
                // idempotent. A collector whose stream ended has to be replaced by one of these.
                await executor.ensureStarted()

                if await (try? condition()) == true { break }

                try? await Task.sleep(for: .milliseconds(1))
            }

            await executor.close()
        }

        func status(_ id: DurableTxId) async throws -> DurableTxStatus? {
            try await store.getEntry(id: id)?.status
        }
    }
}

private extension DurableSubmissionExecutor.Timing {
    /// Real waits, small enough that a retry loop runs in milliseconds and still yields between rounds.
    static let brisk = DurableSubmissionExecutor.Timing(
        initialBackoff: .milliseconds(1),
        maxBackoff: .milliseconds(1),
        rebuildCooldown: .milliseconds(1),
        maxRebuildCooldown: .milliseconds(1),
        maxDoublings: 0,
        clock: ContinuousClock()
    )
}
