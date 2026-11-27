import Foundation
import os

/// Holds the runtime the funding service drives once it is built, and hands
/// it to the calls that waited for it.
final class FundingRuntimeSlot: Sendable {
    private struct State {
        var runtime: FundingRuntime?
        var waiting: [UUID: AsyncStream<FundingRuntime>.Continuation] = [:]
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var runtime: FundingRuntime? {
        state.withLock { $0.runtime }
    }

    func fill(_ runtime: FundingRuntime) {
        let waiting = state.withLock { state in
            state.runtime = runtime
            defer { state.waiting = [:] }
            return Array(state.waiting.values)
        }
        for continuation in waiting {
            continuation.yield(runtime)
            continuation.finish()
        }
    }

    /// The runtime, once it is filled. Throws `CancellationError` when the
    /// calling task is cancelled first.
    func built() async throws -> FundingRuntime {
        let (stream, continuation) = AsyncStream<FundingRuntime>.makeStream()
        let id = UUID()
        continuation.onTermination = { [weak self] _ in
            self?.state.withLock { _ = $0.waiting.removeValue(forKey: id) }
        }
        let filled = state.withLock { state -> FundingRuntime? in
            if state.runtime == nil { state.waiting[id] = continuation }
            return state.runtime
        }
        if let filled {
            continuation.finish()
            return filled
        }
        for await runtime in stream {
            return runtime
        }
        throw CancellationError()
    }
}
