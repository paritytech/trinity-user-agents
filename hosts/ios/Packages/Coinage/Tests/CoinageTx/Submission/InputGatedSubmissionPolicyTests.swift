import AsyncExtensions
import DurableTransactions
import ExtrinsicService
import Foundation
import os
import Testing
@testable import Coinage

/// When the gated policy builds, waits, or gives up — the part every coinage policy shares. The rebuild
/// itself is scripted, so what is under test is purely the decision.
@Suite("Input Gated Submission Policy")
struct InputGatedSubmissionPolicyTests {
    private let now = Date(timeIntervalSince1970: 1_700_000_000)
    private let policyId = SubmissionPolicyId("test-gated")

    // MARK: - Building

    @Test("a transaction whose inputs are all present is built")
    func presentInputsAreBuilt() async throws {
        let scenario = try makeScenario(present: [1], waiting: [Want(inputs: [1], inSeconds: 60)])

        let outcomes = try await scenario.policy.prepareSubmission(scenario.transactions)

        #expect(scenario.outcome(outcomes, 0)?.isReady == true)
        #expect(scenario.rebuild.built == [[1]])
    }

    @Test("a transaction missing an input before its deadline keeps waiting")
    func missingInputBeforeDeadlineWaits() async throws {
        let scenario = try makeScenario(present: [1], waiting: [Want(inputs: [1, 2], inSeconds: 60)])

        let outcomes = try await scenario.policy.prepareSubmission(scenario.transactions)

        // Absent from the result: still waiting, to be asked again.
        #expect(scenario.outcome(outcomes, 0) == nil)
        #expect(scenario.rebuild.built.isEmpty)
    }

    @Test("a transaction whose input is proven gone past its deadline is given up")
    func missingInputPastDeadlineGivesUp() async throws {
        let scenario = try makeScenario(present: [], waiting: [Want(inputs: [1], inSeconds: -1)])

        let outcomes = try await scenario.policy.prepareSubmission(scenario.transactions)

        #expect(scenario.outcome(outcomes, 0)?.isGiveUp == true)
    }

    @Test("a transaction the ledger cannot resolve is given up rather than waited on")
    func unresolvableIsGivenUp() async throws {
        let scenario = try makeScenario(
            present: [1],
            waiting: [Want(inputs: [1], inSeconds: 60)],
            resolvable: false
        )

        let outcomes = try await scenario.policy.prepareSubmission(scenario.transactions)

        #expect(scenario.outcome(outcomes, 0)?.isGiveUp == true)
    }

    @Test("params that cannot be read make a transaction unbuildable, not eternally waiting")
    func unreadableParamsGiveUp() async throws {
        let scenario = try makeScenario(present: [1], waiting: [])
        let corrupt = ScheduledDurableTx(
            id: UUID(),
            domainId: .coinage,
            groupId: nil,
            policy: SubmissionPolicy(id: policyId, params: Data([0xFF]))
        )

        let outcomes = try await scenario.policy.prepareSubmission([corrupt])

        #expect(outcomes[corrupt.id]?.isGiveUp == true)
    }

    // MARK: - A call carrying several transactions

    @Test("the ready ones are built while the rest keep waiting")
    func readyAndWaitingAreDecidedSeparately() async throws {
        let scenario = try makeScenario(present: [1], waiting: [
            Want(inputs: [1], inSeconds: 60),
            Want(inputs: [2], inSeconds: 60)
        ])

        let outcomes = try await scenario.policy.prepareSubmission(scenario.transactions)

        #expect(scenario.outcome(outcomes, 0)?.isReady == true)
        #expect(scenario.outcome(outcomes, 1) == nil)
    }

    @Test("one transaction giving up does not stop another from being built")
    func giveUpDoesNotBlockABuild() async throws {
        let scenario = try makeScenario(present: [1], waiting: [
            Want(inputs: [1], inSeconds: 60),
            Want(inputs: [2], inSeconds: -1)
        ])

        let outcomes = try await scenario.policy.prepareSubmission(scenario.transactions)

        #expect(scenario.outcome(outcomes, 0)?.isReady == true)
        #expect(scenario.outcome(outcomes, 1)?.isGiveUp == true)
    }

    @Test("every transaction in a call is built in one go")
    func aCallBuildsOnce() async throws {
        let scenario = try makeScenario(present: [1, 2], waiting: [
            Want(inputs: [1], inSeconds: 60),
            Want(inputs: [2], inSeconds: 60)
        ])

        _ = try await scenario.policy.prepareSubmission(scenario.transactions)

        #expect(scenario.rebuild.buildCalls == 1)
    }

    @Test("an empty call builds nothing")
    func emptyCallBuildsNothing() async throws {
        let scenario = try makeScenario(present: [1], waiting: [])

        let outcomes = try await scenario.policy.prepareSubmission([])

        #expect(outcomes.isEmpty)
        #expect(scenario.rebuild.built.isEmpty)
    }

    // MARK: - Retrying

    @Test("an expired attempt is retried while the terms allow failures")
    func expiredIsRetried() async throws {
        let scenario = try makeScenario(present: [1], waiting: [])

        let retries = try await scenario.policy.canRetry(
            .fixture(),
            params: transferParams(inSeconds: 60, retryFailures: true),
            failure: .expired
        )

        #expect(retries)
    }

    @Test("terms that do not retry failures decline every kind", arguments: DurableFailureKind.allCases)
    func termsWithoutRetriesDeclineEverything(_ failure: DurableFailureKind) async throws {
        let scenario = try makeScenario(present: [1], waiting: [])

        let retries = try await scenario.policy.canRetry(
            .fixture(),
            params: transferParams(inSeconds: 60, retryFailures: false),
            failure: failure
        )

        #expect(!retries)
    }

    @Test("a failure that would repeat is declined once the window has closed")
    func repeatableFailurePastDeadlineIsDeclined() async throws {
        let scenario = try makeScenario(present: [1], waiting: [])

        let retries = try await scenario.policy.canRetry(
            .fixture(),
            params: transferParams(inSeconds: -1, retryFailures: true),
            failure: .dispatchFailed
        )

        #expect(!retries)
    }

    @Test("params that cannot be read decline the retry rather than guessing")
    func unreadableParamsDeclineRetry() async throws {
        let scenario = try makeScenario(present: [1], waiting: [])

        let retries = await scenario.policy.canRetry(.fixture(), params: Data([0xFF]), failure: .expired)

        #expect(!retries)
    }
}

// MARK: - Support

private extension InputGatedSubmissionPolicyTests {
    /// One transaction to schedule: the inputs it spends and how far off its deadline is.
    struct Want {
        let inputs: Set<Int>
        let inSeconds: TimeInterval
    }

    /// A policy over a rebuild scripted for exactly the transactions the test scheduled. Everything the
    /// rebuild answers is held per scenario, so parallel tests share nothing.
    struct Scenario {
        let policy: InputGatedSubmissionPolicy<ScriptedRebuild>
        let rebuild: ScriptedRebuild
        let transactions: [ScheduledDurableTx]

        func outcome(
            _ outcomes: [DurableTxId: SubmissionPreparation],
            _ index: Int
        ) -> SubmissionPreparation? {
            outcomes[transactions[index].id]
        }
    }

    func makeScenario(present: Set<Int>, waiting: [Want], resolvable: Bool = true) throws -> Scenario {
        var transactions: [ScheduledDurableTx] = []
        var inputs: [CoinageTxId: Set<Int>] = [:]

        for want in waiting {
            let transaction = try ScheduledDurableTx(
                id: UUID(),
                domainId: .coinage,
                groupId: nil,
                policy: SubmissionPolicy(
                    id: policyId,
                    params: transferParams(inSeconds: want.inSeconds, retryFailures: true)
                )
            )
            transactions.append(transaction)
            inputs[transaction.id] = want.inputs
        }

        let rebuild = ScriptedRebuild(present: present, inputs: inputs, resolvable: resolvable)

        return Scenario(
            policy: InputGatedSubmissionPolicy(
                policyId: policyId,
                chainId: "test-chain",
                rebuild: rebuild,
                ledger: StubAssetLedger(),
                // Both bound the pathological case only: every scripted presence stream yields once and
                // finishes, so a wait ends on the look or on the stream, never on a timeout. They are
                // generous so that a stalled runner cannot turn "the look had not arrived yet" into a
                // verdict the test then reads as wrong.
                timing: InputWaitTiming(
                    holdOut: .seconds(60),
                    idleLimit: .seconds(60),
                    dateProvider: StubDateProvider(now)
                ),
                logger: nil
            ),
            rebuild: rebuild,
            transactions: transactions
        )
    }

    func transferParams(inSeconds: TimeInterval, retryFailures: Bool) throws -> Data {
        try CoinageSubmissionParams.splitPolicy(
            TransferSubmissionParams(
                buildUntil: now.addingTimeInterval(inSeconds),
                retryFailures: retryFailures
            )
        ).params
    }
}

/// A ``CoinageRebuild`` whose resolution, inputs and chain presence are all stated by the test, so the
/// policy's decisions are what is being exercised.
final class ScriptedRebuild: CoinageRebuild, @unchecked Sendable {
    struct Transaction: Sendable {
        let id: CoinageTxId
        let inputs: Set<Int>
    }

    private struct Recording {
        var built: [Set<Int>] = []
        var buildCalls = 0
    }

    private let present: Set<Int>
    private let inputsById: [CoinageTxId: Set<Int>]
    private let resolvable: Bool
    private let recording = OSAllocatedUnfairLock(initialState: Recording())

    init(present: Set<Int>, inputs: [CoinageTxId: Set<Int>], resolvable: Bool = true) {
        self.present = present
        inputsById = inputs
        self.resolvable = resolvable
    }

    /// The inputs of each transaction handed to `build`, in order.
    var built: [Set<Int>] {
        recording.withLock { $0.built }
    }

    var buildCalls: Int {
        recording.withLock { $0.buildCalls }
    }

    func terms(of params: Data) -> RebuildTerms? {
        guard let transfer = try? CoinageSubmissionParams.decodeTransfer(params) else { return nil }

        return RebuildTerms(deadline: transfer.buildUntil, retriesFailures: transfer.retryFailures)
    }

    func resolve(
        _ transactions: [ScheduledDurableTx],
        assets _: [CoinageTxId: CoinageTxEntry]
    ) async throws -> [CoinageTxId: Transaction] {
        guard resolvable else { return [:] }

        return transactions.reduce(into: [:]) { resolved, transaction in
            resolved[transaction.id] = Transaction(
                id: transaction.id,
                inputs: inputsById[transaction.id] ?? []
            )
        }
    }

    func inputs(of transaction: Transaction) -> Set<Int> {
        transaction.inputs
    }

    func presence(of _: Set<Int>) async throws -> AnyAsyncSequence<Set<Int>> {
        let present = present

        return AsyncStream<Set<Int>> { continuation in
            continuation.yield(present)
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }

    func build(_ transactions: [Transaction]) async throws -> [ExtrinsicBuiltModel] {
        recording.withLock { current in
            current.buildCalls += 1
            current.built.append(contentsOf: transactions.map(\.inputs))
        }

        return transactions.map { .fixture(payload: $0.id.uuidString) }
    }
}

/// Only `assets(of:)` is reached by the gated policy; the rest of the ledger is out of its path.
private struct StubAssetLedger: CoinageAssetLedgerProtocol {
    func registerAssets(
        _: [CoinageAssetRegistration],
        for _: [CoinageTxId],
        in _: any DurableTxRegistrationScope
    ) throws {}

    func getAllEntries() async throws -> [CoinageTxEntry] { [] }
    func getEntry(id _: CoinageTxId) async throws -> CoinageTxEntry? { nil }
    func assets(of _: [CoinageTxId]) async throws -> [CoinageTxId: CoinageTxEntry] { [:] }
    func getOperationGroupStatuses(_: CoinageTxGroupId) async throws -> [CoinageTxEntry] { [] }

    func subscribeOperationGroupStatuses(_: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]> {
        AsyncStream<[CoinageTxEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func precommitHandOff(
        _: [OwnAsset],
        validation _: @escaping (any CoinageTxValidationContextProtocol) throws -> Void
    ) async throws {}

    func releaseUncommittedHandoffs() async throws {}
    func releaseUncommittedHandoffs(_: [PublicKey]) async throws {}
    func commitHandoffs(_: [PublicKey], in _: any DurableTxRegistrationScope) throws {}
    func handedOffCoins() async throws -> [OwnAsset] { [] }
}

extension SubmissionPreparation {
    var isReady: Bool {
        if case .ready = self { return true }
        return false
    }

    var isGiveUp: Bool {
        if case .giveUp = self { return true }
        return false
    }
}
