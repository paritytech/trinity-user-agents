import AsyncExtensions
import Foundation
import os
@testable import Coinage

/// A counter a test can suspend on until it reaches a value, instead of polling.
final class CountSignal: @unchecked Sendable {
    private struct Waiter {
        let count: Int
        let continuation: CheckedContinuation<Void, Never>
    }

    private let state = OSAllocatedUnfairLock(initialState: (count: 0, waiters: [Waiter]()))

    var count: Int { state.withLock { $0.count } }

    func increment() {
        let ready: [Waiter] = state.withLock { state in
            state.count += 1
            let reached = state.count
            let ready = state.waiters.filter { $0.count <= reached }
            state.waiters.removeAll { $0.count <= reached }
            return ready
        }

        ready.forEach { $0.continuation.resume() }
    }

    func wait(for count: Int) async {
        await withCheckedContinuation { continuation in
            let reached = state.withLock { state in
                guard state.count < count else { return true }

                state.waiters.append(Waiter(count: count, continuation: continuation))

                return false
            }

            if reached {
                continuation.resume()
            }
        }
    }
}

/// Coin-info subscriptions the test drives: each `subscribeCoinInfos` opens a stream that stays silent
/// until the test sends a look into it or fails it.
final class ScriptedCoinInfoQuery: CoinOnChainQuerying, @unchecked Sendable {
    struct Unsupported: Error {}
    struct Dropped: Error {}

    private let subscriptions = OSAllocatedUnfairLock(
        initialState: [AsyncThrowingStream<[Data: ClaimableCoinInfo], Error>.Continuation]()
    )
    private let opened = CountSignal()

    var subscribeCalls: Int { opened.count }

    func waitForSubscriptions(_ count: Int) async {
        await opened.wait(for: count)
    }

    /// Delivers `look` on the latest subscription.
    func send(_ look: [Data: ClaimableCoinInfo]) {
        subscriptions.withLock { $0.last }?.yield(look)
    }

    /// Ends the latest subscription with an error.
    func fail() {
        subscriptions.withLock { $0.last }?.finish(throwing: Dropped())
    }

    func subscribeCoinInfos(for _: [Data]) -> AnyAsyncSequence<[Data: ClaimableCoinInfo]> {
        let (stream, continuation) = AsyncThrowingStream<[Data: ClaimableCoinInfo], Error>.makeStream()
        subscriptions.withLock { $0.append(continuation) }
        opened.increment()

        return stream.eraseToAnyAsyncSequence()
    }

    func fetchCoins(for _: [Data], atBlockHash _: Data?) async throws -> [CoinSyncResult.OnChainCoin?] {
        throw Unsupported()
    }

    func awaitAllCoinsOnChain(for _: [Data]) async throws {
        throw Unsupported()
    }

    func awaitAllCoinsOffChain(for _: [Data]) async throws {
        throw Unsupported()
    }
}

/// Records every claim registration; registers nothing, so the claim group stays empty.
final class RecordingClaimSubmitter: CoinageClaimSubmitting, @unchecked Sendable {
    private let submitted = OSAllocatedUnfairLock(initialState: [[Data]]())
    private let submissions = CountSignal()

    var submittedPublicKeys: [[Data]] { submitted.withLock { $0 } }

    func waitForSubmissions(_ count: Int) async {
        await submissions.wait(for: count)
    }

    func submit(
        claimable: [ClaimableCoin],
        bundleSize _: Int,
        groupId _: CoinageTxGroupId,
        retryUntil _: Date
    ) async throws {
        submitted.withLock { $0.append(claimable.map(\.publicKey)) }
        submissions.increment()
    }
}
