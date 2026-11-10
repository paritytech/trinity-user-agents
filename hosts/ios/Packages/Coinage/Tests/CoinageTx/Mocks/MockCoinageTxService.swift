import Foundation
import os
import Operation_iOS
import ExtrinsicService
import AsyncExtensions
@testable import Coinage
import DurableTransactions

/// Thread-safe journal for recording mock call events.
final class CallJournal: @unchecked Sendable {
    private let mutex = OSAllocatedUnfairLock<State>(initialState: State())

    private struct State {
        var events: [String] = []
    }

    func record(_ event: String) {
        mutex.withLock { $0.events.append(event) }
    }

    var events: [String] {
        mutex.withLock { $0.events }
    }
}

actor MockCoinageTxService: CoinageTxServicing {
    let store: MockCoinageTxRepository
    let callJournal: CallJournal

    /// One record of what a strategy declared, however it was registered.
    ///
    /// Submitted and scheduled transactions land here alike: an assertion about what a transfer
    /// consumes and mints holds either way, and a test that cares which path was taken reads
    /// ``scheduledRequests``. Lock-backed rather than actor state because `scheduleTransactions` is a
    /// synchronous requirement — it runs inside a store's write block, which cannot suspend.
    private struct Recorded {
        var inputs: [[CoinageTxInput]] = []
        var outputs: [[OwnAsset]] = []
        var scheduled: [CoinageScheduledTxRequest] = []
    }

    private nonisolated let recorded = OSAllocatedUnfairLock<Recorded>(initialState: Recorded())

    nonisolated var submittedInputs: [[CoinageTxInput]] { recorded.withLock { $0.inputs } }
    nonisolated var submittedOutputs: [[OwnAsset]] { recorded.withLock { $0.outputs } }
    private(set) var handoffAssets: [OwnAsset] = []

    private let submissionOutcome: SubmissionOutcome

    enum SubmissionOutcome {
        /// Registration succeeds and the entry resolves to `finalizedSuccess`.
        case success
        /// Registration succeeds and the entry resolves to `failure`.
        case chainFailure
        /// `submit` throws before registering.
        case thrown
    }

    init(
        store: MockCoinageTxRepository = MockCoinageTxRepository(),
        callJournal: CallJournal = CallJournal(),
        submissionOutcome: SubmissionOutcome = .success
    ) {
        self.store = store
        self.callJournal = callJournal
        self.submissionOutcome = submissionOutcome
    }

    @discardableResult
    func submitTransactions(
        _ requests: [CoinageTxRequest],
        groupId: CoinageTxGroupId?
    ) async throws -> [CoinageTxId] {
        var ids: [CoinageTxId] = []
        for request in requests {
            try await ids.append(recordSubmission(request, groupId: groupId))
        }
        return ids
    }

    private func recordSubmission(_ request: CoinageTxRequest, groupId: CoinageTxGroupId?) async throws -> CoinageTxId {
        recorded.withLock { current in
            current.inputs.append(request.inputs)
            current.outputs.append(request.outputs)
        }
        callJournal.record("submit")

        if case .thrown = submissionOutcome {
            throw StubError.boom
        }

        let entry = CoinageTxEntry(
            inputs: request.inputs,
            outputs: request.outputs,
            groupId: groupId,
            txHash: Data(repeating: 0xAB, count: 32),
            checkpoint: BlockRef(number: 0, hash: Data(repeating: 0, count: 32)),
            mortality: 300
        )
        try await store.register(entry)

        // Drive the entry to a terminal status so a caller awaiting the outcome via
        // `subscribeTransactionStatus` resolves immediately.
        let terminal: CoinageTxStatus =
            switch submissionOutcome {
            case .chainFailure: .failure
            case .success,
                 .thrown: .finalizedSuccess
            }
        try await store.updateStatus(entry.id, to: terminal)

        return entry.id
    }

    /// What a caller scheduled, so a test can assert the transactions a strategy declared.
    ///
    /// `nonisolated` because the protocol requirement is synchronous — it runs inside a store's write
    /// block, which cannot suspend — so an actor cannot satisfy it from isolated state.
    nonisolated var scheduledRequests: [CoinageScheduledTxRequest] {
        recorded.withLock { $0.scheduled }
    }

    @discardableResult
    nonisolated func scheduleTransactions(
        _ requests: [CoinageScheduledTxRequest],
        groupId: CoinageTxGroupId,
        joining _: any DurableTxRegistrationScope
    ) throws -> [CoinageTxId] {
        recorded.withLock { current in
            current.scheduled.append(contentsOf: requests)
            current.inputs.append(contentsOf: requests.map(\.inputs))
            current.outputs.append(contentsOf: requests.map(\.outputs))
        }
        callJournal.record("schedule")

        let ids = requests.map { _ in CoinageTxId() }

        Task { [store] in
            for request in requests {
                let entry = CoinageTxEntry(
                    inputs: request.inputs,
                    outputs: request.outputs,
                    groupId: groupId,
                    txHash: Data(repeating: 0xAB, count: 32),
                    checkpoint: BlockRef(number: 0, hash: Data(repeating: 0, count: 32)),
                    mortality: 300
                )
                try? await store.register(entry)
            }
        }

        return ids
    }

    nonisolated func subscribeTransactionStatus(_ id: CoinageTxId) -> AnyAsyncSequence<CoinageTxStatus> {
        store.subscribeStatus(id: id)
    }

    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry] {
        try await store.getOperationGroupStatuses(groupId)
    }

    nonisolated func subscribeOperationGroupStatuses(
        _ groupId: CoinageTxGroupId
    ) -> AnyAsyncSequence<[CoinageTxEntry]> {
        store.subscribeOperationGroupStatuses(groupId)
    }

    func preCommitHandoff(_ assets: [OwnAsset]) async throws -> any CoinageHandoffCommit {
        callJournal.record("preCommitHandoff")
        handoffAssets.append(contentsOf: assets)
        try await store.precommitHandOff(assets) { _ in }
        return StoreHandoffCommit(assets: assets, ledger: store.ledger)
    }

    func releaseUncommittedHandoffs() async throws {
        try await store.releaseUncommittedHandoffs()
    }
}

enum StubError: Error {
    case boom
}
