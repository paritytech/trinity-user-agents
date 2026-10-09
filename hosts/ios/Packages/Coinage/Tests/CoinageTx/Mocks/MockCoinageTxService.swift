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
    private let beforeRegistration: (@Sendable () async throws -> Void)?

    enum SubmissionOutcome: Equatable {
        /// Registration succeeds and the entry resolves to `finalizedSuccess`.
        case success
        /// Registration succeeds and the entry resolves to `failure`.
        case chainFailure
        /// `submit` throws before registering.
        case thrown
        /// Durable custody committed, but the caller never receives the submission result.
        case registeredThenThrown
    }

    init(
        store: MockCoinageTxRepository = MockCoinageTxRepository(),
        callJournal: CallJournal = CallJournal(),
        submissionOutcome: SubmissionOutcome = .success,
        beforeRegistration: (@Sendable () async throws -> Void)? = nil
    ) {
        self.store = store
        self.callJournal = callJournal
        self.submissionOutcome = submissionOutcome
        self.beforeRegistration = beforeRegistration
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

    /// Native scheduling registers through the real in-memory store and ledger, so custody, marks and
    /// transaction rows commit or roll back together, then resolves each row like a submission.
    @discardableResult
    func scheduleTransactions(
        _ requests: [CoinageScheduledTxRequest],
        groupId: CoinageTxGroupId?,
        custody: NativeTransferCustody,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws -> [CoinageTxId] {
        callJournal.record("schedule")
        if case .thrown = submissionOutcome { throw StubError.boom }
        try await beforeRegistration?()
        let ledger = store.ledger
        let assets = requests.map { CoinageAssetRegistration(inputs: $0.inputs, outputs: $0.outputs) }
        let ids = try await store.durable.schedule(
            requests.map { DurableTxSchedule(domainId: .coinage, groupId: groupId, policy: $0.policy) },
            in: nil
        ) { scope, ids in
            try ledger.registerAssets(assets, for: ids, custody: custody, authorization: authorization, in: scope)
        }
        recorded.withLock { current in
            current.scheduled.append(contentsOf: requests)
            current.inputs.append(contentsOf: requests.map(\.inputs))
            current.outputs.append(contentsOf: requests.map(\.outputs))
        }
        handoffAssets += custody.assets
        if case .registeredThenThrown = submissionOutcome { throw StubError.boom }
        for id in ids {
            try await store.updateStatus(id, to: submissionOutcome == .chainFailure ? .failure : .finalizedSuccess)
        }
        return ids
    }

    func retainNativeTransfer(
        _ custody: NativeTransferCustody, authorization: @escaping @Sendable () throws -> Void
    ) async throws -> any CoinageHandoffCommit {
        try await store.ledger.retainNativeTransfer(custody, authorization: authorization)
        handoffAssets += custody.assets
        return StoreHandoffCommit(assets: custody.assets, ledger: store.ledger)
    }

    func retainedNativeTransfer(
        custodyId: String
    ) async throws -> (custody: NativeTransferCustody, handoffCommit: any CoinageHandoffCommit)? {
        guard let custody = try await store.ledger.retainedNativeTransfer(custodyId: custodyId) else { return nil }
        return (custody, StoreHandoffCommit(assets: custody.assets, ledger: store.ledger))
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
        if case .registeredThenThrown = submissionOutcome { throw StubError.boom }

        // Drive the entry to a terminal status so a caller awaiting the outcome via
        // `subscribeTransactionStatus` resolves immediately.
        let terminal: CoinageTxStatus =
            switch submissionOutcome {
            case .chainFailure: .failure
            case .success,
                 .thrown, .registeredThenThrown: .finalizedSuccess
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
        try await store.precommitHandOff(assets) { context in
            try CoinageTxRegistrationValidator().validateHandoff(Set(assets.map(\.publicKey)), transaction: context)
        }
        return StoreHandoffCommit(assets: assets, ledger: store.ledger)
    }

    func releaseUncommittedHandoffs() async throws {
        try await store.releaseUncommittedHandoffs()
    }
}

enum StubError: Error {
    case boom
}
