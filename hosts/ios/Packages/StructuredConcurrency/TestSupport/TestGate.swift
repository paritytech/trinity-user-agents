import Foundation
import os

/// Holds callers of `pass()` while closed. A cancelled caller leaves with `CancellationError`, as a real
/// request would, so a timeout around the call can end it.
public final class TestGate: Sendable {
    fileprivate struct Observer {
        let count: Int
        let continuation: CheckedContinuation<Void, Never>
    }

    fileprivate struct State {
        var isClosed: Bool
        var parked: [UUID: CheckedContinuation<Void, Error>] = [:]
        var cancelledIds: Set<UUID> = []
        var observers: [Observer] = []
    }

    fileprivate enum Arrival {
        case passed
        case cancelled
        case parked(ready: [Observer])
    }

    private let state: OSAllocatedUnfairLock<State>

    public init(isClosed: Bool = true) {
        state = OSAllocatedUnfairLock(initialState: State(isClosed: isClosed))
    }

    /// Later callers park.
    public func close() {
        state.withLock { $0.isClosed = true }
    }

    /// Later callers pass. Callers already parked stay parked until `release()`.
    public func open() {
        state.withLock { $0.isClosed = false }
    }

    /// Resumes everything parked so far. Does not change whether later callers park.
    public func release() {
        let parked = state.withLock { state in
            defer { state.parked = [:] }
            return Array(state.parked.values)
        }

        parked.forEach { $0.resume() }
    }

    public func pass() async throws {
        let id = UUID()

        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                arrive(id: id, continuation: continuation)
            }
        } onCancel: {
            cancel(id: id)
        }
    }

    /// Suspends until at least `count` callers are parked.
    public func waitForParked(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.parked.count < count else { return true }

                state.observers.append(Observer(count: count, continuation: continuation))

                return false
            }

            if isReached {
                continuation.resume()
            }
        }
    }
}

private extension TestGate {
    func arrive(id: UUID, continuation: CheckedContinuation<Void, Error>) {
        let arrival: Arrival = state.withLock { state in
            guard state.cancelledIds.remove(id) == nil else { return .cancelled }
            guard state.isClosed else { return .passed }

            state.parked[id] = continuation

            let parkedCount = state.parked.count
            let ready = state.observers.filter { $0.count <= parkedCount }
            state.observers.removeAll { $0.count <= parkedCount }

            return .parked(ready: ready)
        }

        switch arrival {
        case .passed:
            continuation.resume()
        case .cancelled:
            continuation.resume(throwing: CancellationError())
        case let .parked(ready):
            ready.forEach { $0.continuation.resume() }
        }
    }

    /// Cancellation can arrive before the caller parks, so an unknown id is remembered for `arrive`.
    func cancel(id: UUID) {
        let continuation: CheckedContinuation<Void, Error>? = state.withLock { state in
            guard let continuation = state.parked.removeValue(forKey: id) else {
                state.cancelledIds.insert(id)
                return nil
            }

            return continuation
        }

        continuation?.resume(throwing: CancellationError())
    }
}
