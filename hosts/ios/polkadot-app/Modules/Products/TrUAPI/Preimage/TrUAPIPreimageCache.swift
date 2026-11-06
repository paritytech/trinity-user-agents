import Foundation
import os
import StructuredConcurrency

/// Awaitable preimage lookup seam consumed by the rust bridge.
public protocol TrUAPIPreimageLookuping: Sendable {
    func lookup(key: Data) async -> Data?
}

/// Awaitable preimage lookup: cache hit returns immediately; a miss awaits
/// one coalesced fetch shared by all concurrent callers (per key, via
/// CoalescingTask). Failed fetches are not cached, so a later lookup retries.
public final class TrUAPIPreimageCache: TrUAPIPreimageLookuping, @unchecked Sendable {
    private struct State {
        var cache: [Data: Data] = [:]
        var coalescers: [Data: CoalescingTask<Data?>] = [:]
    }

    private let state = OSAllocatedUnfairLock<State>(initialState: State())

    /// Closure that performs the actual remote lookup for a given raw hash.
    /// Returns nil when the value cannot be fetched.
    private let fetchValue: @Sendable (Data) async -> Data?

    public init(fetchValue: @escaping @Sendable (Data) async -> Data?) {
        self.fetchValue = fetchValue
    }

    public func lookup(key: Data) async -> Data? {
        if let cached = state.withLock({ $0.cache[key] }) {
            return cached
        }

        let coalescer = state.withLock { state in
            if let existing = state.coalescers[key] {
                return existing
            }
            let coalescer = CoalescingTask<Data?>()
            state.coalescers[key] = coalescer
            return coalescer
        }

        let value = await (try? coalescer.run { [fetchValue] in await fetchValue(key) }) ?? nil

        if let value {
            state.withLock { $0.cache[key] = value }
        }
        return value
    }
}
