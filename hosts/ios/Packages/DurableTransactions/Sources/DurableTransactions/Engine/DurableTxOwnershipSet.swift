import Foundation
import os

/// The set of transactions a live submission currently owns, so the recovery pass skips exactly those
/// and each transaction has one writer at a time.
///
/// Ownership is one-shot **per attempt**: an attempt is taken once, released once, and never taken
/// again. A rebuilt transaction is a different attempt with a different hash, so it is owned afresh —
/// which is what lets a row be watched again without ever re-owning bytes that were already judged.
/// A pass reads this per transaction so one registered mid-pass is skipped rather than judged on a
/// chain view older than it is.
///
/// Deliberately volatile: a crash takes the set with it, and an empty set after restart is the correct
/// answer — a durable set would strand transactions behind a watcher that no longer exists.
///
/// Synchronous by design: the pass must test membership without suspending between the test and the
/// read it guards.
public final class DurableTxOwnershipSet: Sendable {
    private struct State {
        var owned: [DurableTxId: Data] = [:]
        var everReleased: Set<Attempt> = []
    }

    private struct Attempt: Hashable {
        let id: DurableTxId
        let txHash: Data
    }

    private let lock = OSAllocatedUnfairLock(initialState: State())

    public init() {}

    /// Takes ownership of one attempt of a transaction. Returns whether ownership was taken: an attempt
    /// whose bytes were already watched and released is never owned again, so a caller that loses this
    /// race must not start it.
    @discardableResult
    public func take(_ id: DurableTxId, txHash: Data) -> Bool {
        lock.withLock { state in
            guard !state.everReleased.contains(Attempt(id: id, txHash: txHash)) else {
                return false
            }

            state.owned[id] = txHash

            return true
        }
    }

    /// Releases ownership of one attempt. Returns false if that attempt was already released, so a caller
    /// can keep release side effects one-shot.
    @discardableResult
    public func release(_ id: DurableTxId, txHash: Data) -> Bool {
        lock.withLock { state in
            if state.owned[id] == txHash {
                state.owned[id] = nil
            }

            return state.everReleased.insert(Attempt(id: id, txHash: txHash)).inserted
        }
    }

    public func isOwned(_ id: DurableTxId) -> Bool {
        lock.withLock { state in state.owned[id] != nil }
    }
}
