import AsyncExtensions
import Foundation
import os
@testable import Coinage
import DurableTransactions

/// `CoinageTxServicing` double with real group semantics: registrations are keyed by group id,
/// resolved to a configured terminal status, and re-readable / re-subscribable — the seam the
/// offboarding re-join and the restart scenario depend on.
final class StubGroupTxService: CoinageTxServicing, @unchecked Sendable {
    enum Outcome {
        case success
        case failure
        /// First entry finalizes, the rest fail.
        case partial
    }

    struct Failure: Error {}

    private struct State {
        var groups: [CoinageTxGroupId: [CoinageTxEntry]] = [:]
        var subjects: [CoinageTxGroupId: AsyncCurrentValueSubject<[CoinageTxEntry]>] = [:]
        var registrations: [CoinageTxGroupId] = []
        var outcome: Outcome = .success
        var outcomeQueue: [Outcome] = []
        var submitError: Error?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var registrations: [CoinageTxGroupId] {
        state.withLock { $0.registrations }
    }

    func setOutcome(_ outcome: Outcome) {
        state.withLock { $0.outcome = outcome }
    }

    /// One outcome per registration, in order; falls back to `setOutcome` once drained.
    func setOutcomes(_ outcomes: [Outcome]) {
        state.withLock { $0.outcomeQueue = outcomes }
    }

    func setSubmitError(_ error: Error?) {
        state.withLock { $0.submitError = error }
    }

    /// Pre-registers a group as a prior run would have left it.
    func seedGroup(_ groupId: CoinageTxGroupId, statuses: [CoinageTxStatus]) {
        let entries = statuses.map { status in
            CoinageTxEntry(
                inputs: [],
                outputs: [],
                groupId: groupId,
                txHash: Data(repeating: 0xAB, count: 32),
                checkpoint: BlockRef(number: 0, hash: Data(repeating: 0, count: 32)),
                mortality: 300,
                status: status
            )
        }
        store(entries, for: groupId)
    }

    @discardableResult
    func submitTransactions(
        _ requests: [CoinageTxRequest],
        groupId: CoinageTxGroupId?
    ) async throws -> [CoinageTxId] {
        let (error, outcome) = state.withLock { state -> (Error?, Outcome) in
            let next = state.outcomeQueue.isEmpty ? state.outcome : state.outcomeQueue.removeFirst()
            return (state.submitError, next)
        }
        if let error { throw error }

        let entries = requests.enumerated().map { index, request in
            CoinageTxEntry(
                inputs: request.inputs,
                outputs: request.outputs,
                groupId: groupId,
                txHash: Data(repeating: 0xAB, count: 32),
                checkpoint: BlockRef(number: 0, hash: Data(repeating: 0, count: 32)),
                mortality: 300,
                status: Self.status(for: outcome, index: index)
            )
        }

        if let groupId {
            state.withLock { $0.registrations.append(groupId) }
            store(entries, for: groupId)
        }

        return entries.map(\.id)
    }

    /// The offramp never schedules — it registers built extrinsics and is never retried.
    @discardableResult
    func scheduleTransactions(
        _: [CoinageScheduledTxRequest],
        groupId _: CoinageTxGroupId,
        joining _: any DurableTxRegistrationScope
    ) throws -> [CoinageTxId] {
        []
    }

    func subscribeTransactionStatus(_: CoinageTxId) -> AnyAsyncSequence<CoinageTxStatus> {
        AsyncStream<CoinageTxStatus> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry] {
        state.withLock { $0.groups[groupId] ?? [] }
    }

    func subscribeOperationGroupStatuses(_ groupId: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]> {
        subject(for: groupId).eraseToAnyAsyncSequence()
    }

    func preCommitHandoff(_: [OwnAsset]) async throws -> any CoinageHandoffCommit {
        throw Failure()
    }

    func releaseUncommittedHandoffs() async throws {}
}

private extension StubGroupTxService {
    static func status(for outcome: Outcome, index: Int) -> CoinageTxStatus {
        switch outcome {
        case .success: .finalizedSuccess
        case .failure: .failure
        case .partial: index == 0 ? .finalizedSuccess : .failure
        }
    }

    func subject(for groupId: CoinageTxGroupId) -> AsyncCurrentValueSubject<[CoinageTxEntry]> {
        state.withLock { state in
            if let existing = state.subjects[groupId] { return existing }
            let subject = AsyncCurrentValueSubject<[CoinageTxEntry]>(state.groups[groupId] ?? [])
            state.subjects[groupId] = subject
            return subject
        }
    }

    func store(_ entries: [CoinageTxEntry], for groupId: CoinageTxGroupId) {
        state.withLock { $0.groups[groupId] = entries }
        subject(for: groupId).send(entries)
    }
}
