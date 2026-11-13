import Foundation
import os

/// Hands out the one funding service, which is built with the runtime.
protocol FundingServiceProviding: AnyObject, Sendable {
    /// The service, building the runtime first when it is not built yet.
    func fundingService() throws -> FundingServiceProtocol

    /// Yields the service once the runtime is built, by whoever builds it,
    /// then ends. Does not build the runtime.
    func builtFundingService() -> AsyncStream<FundingServiceProtocol>
}

/// Holds the funding service once it is built, and hands it to those who
/// asked before.
final class FundingServiceSlot: Sendable {
    private struct State {
        var service: FundingServiceProtocol?
        var waiting: [UUID: AsyncStream<FundingServiceProtocol>.Continuation] = [:]
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var service: FundingServiceProtocol? {
        state.withLock { $0.service }
    }

    func fill(_ service: FundingServiceProtocol) {
        let waiting = state.withLock { state in
            state.service = service
            defer { state.waiting = [:] }
            return Array(state.waiting.values)
        }
        for continuation in waiting {
            continuation.yield(service)
            continuation.finish()
        }
    }

    func built() -> AsyncStream<FundingServiceProtocol> {
        let (stream, continuation) = AsyncStream<FundingServiceProtocol>.makeStream()
        let id = UUID()
        continuation.onTermination = { [weak self] _ in
            self?.state.withLock { _ = $0.waiting.removeValue(forKey: id) }
        }
        let built = state.withLock { state -> FundingServiceProtocol? in
            if state.service == nil { state.waiting[id] = continuation }
            return state.service
        }
        if let built {
            continuation.yield(built)
            continuation.finish()
        }
        return stream
    }
}
