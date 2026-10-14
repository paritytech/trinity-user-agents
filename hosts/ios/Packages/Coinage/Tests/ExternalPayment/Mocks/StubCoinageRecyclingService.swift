import AsyncExtensions
import Foundation
import os
@testable import Coinage

/// Records recycle submissions per group and answers `observeRecycling` from a script per group.
/// Groups listed in `existingGroups` behave as already registered: the submission is skipped.
actor StubCoinageRecyclingService: CoinageRecyclingServicing {
    struct Submission: Equatable {
        let coins: [Coin]
        let groupId: CoinageTxGroupId?
    }

    private struct State {
        var submissions: [Submission] = []
        var existingGroups: Set<CoinageTxGroupId> = []
        var scripts: [CoinageTxGroupId: [RecyclingStatus]] = [:]
        var error: Error?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    nonisolated var submissions: [Submission] {
        state.withLock { $0.submissions }
    }

    nonisolated func setError(_ error: Error?) {
        state.withLock { $0.error = error }
    }

    nonisolated func markExisting(_ groupId: CoinageTxGroupId) {
        state.withLock { _ = $0.existingGroups.insert(groupId) }
    }

    /// Statuses `observeRecycling(groupId:)` yields, in order, before finishing.
    nonisolated func script(groupId: CoinageTxGroupId, statuses: [RecyclingStatus]) {
        state.withLock { $0.scripts[groupId] = statuses }
    }

    @discardableResult
    func recycleCoins(_ coins: [Coin], groupId: CoinageTxGroupId?) async throws -> Int {
        try state.withLock { state in
            if let error = state.error { throw error }
            if let groupId, state.existingGroups.contains(groupId) { return 0 }
            state.submissions.append(Submission(coins: coins, groupId: groupId))
            if let groupId { state.existingGroups.insert(groupId) }
            return coins.count
        }
    }

    nonisolated func observeRecycling(groupId: CoinageTxGroupId) -> AnyAsyncSequence<RecyclingStatus> {
        let statuses = state.withLock { $0.scripts[groupId] ?? [] }

        return AsyncStream<RecyclingStatus> { continuation in
            for status in statuses {
                continuation.yield(status)
            }
            continuation.finish()
        }
        .eraseToAnyAsyncSequence()
    }
}
