import Foundation
import os
import Testing

/// Sleeps finish only when the test resumes them by duration; `now` stays zero, so elapsed-time reads see none.
public final class ManualClock: Clock, Sendable {
    public struct Instant: InstantProtocol {
        public let offset: Duration

        public func advanced(by duration: Duration) -> Self {
            Self(offset: offset + duration)
        }

        public func duration(to other: Self) -> Duration {
            other.offset - offset
        }

        public static func < (lhs: Self, rhs: Self) -> Bool {
            lhs.offset < rhs.offset
        }
    }

    fileprivate struct Sleeper {
        let id: UUID
        let duration: Duration
        let continuation: CheckedContinuation<Void, Error>
    }

    fileprivate struct Waiter {
        let duration: Duration
        let count: Int
        let continuation: CheckedContinuation<Void, Never>
    }

    fileprivate struct State {
        var sleepers: [Sleeper] = []
        var waiters: [Waiter] = []
        var cancelledIds: Set<UUID> = []
    }

    public let now = Instant(offset: .zero)
    public let minimumResolution: Duration = .zero

    private let state = OSAllocatedUnfairLock(initialState: State())

    public init() {}

    public func sleep(until deadline: Instant, tolerance _: Duration? = nil) async throws {
        let id = UUID()
        let duration = now.duration(to: deadline)

        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                register(Sleeper(id: id, duration: duration, continuation: continuation))
            }
        } onCancel: {
            cancel(id: id)
        }
    }

    /// Suspends until a sleep of `duration` is pending, without finishing it.
    public func waitForSleep(for duration: Duration) async {
        await waitForSleeps(for: duration, count: 1)
    }

    /// Suspends until at least `count` sleeps of `duration` are pending, without finishing them.
    public func waitForSleeps(for duration: Duration, count: Int) async {
        await withCheckedContinuation { continuation in
            let isPending = state.withLock { state in
                guard state.pendingCount(of: duration) < count else { return true }

                state.waiters.append(Waiter(duration: duration, count: count, continuation: continuation))

                return false
            }

            if isPending {
                continuation.resume()
            }
        }
    }

    /// Waits for a sleep of `duration` to be pending, then finishes it.
    public func resumeSleep(
        for duration: Duration,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async {
        await resumeSleeps(for: duration, count: 1, sourceLocation: sourceLocation)
    }

    /// Waits for `count` sleeps of `duration` to be pending, then finishes them together.
    public func resumeSleeps(
        for duration: Duration,
        count: Int,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async {
        await waitForSleeps(for: duration, count: count)

        let sleepers: [Sleeper] = state.withLock { state in
            let matching = state.sleepers.filter { $0.duration == duration }.prefix(count)
            let ids = Set(matching.map(\.id))
            state.sleepers.removeAll { ids.contains($0.id) }

            return Array(matching)
        }

        if sleepers.count < count {
            Issue.record(
                "\(count - sleepers.count) of \(count) \(duration) sleeps were cancelled before they could be resumed",
                sourceLocation: sourceLocation
            )
        }

        sleepers.forEach { $0.continuation.resume() }
    }
}

private extension ManualClock {
    func register(_ sleeper: Sleeper) {
        let (isCancelled, readyWaiters): (Bool, [Waiter]) = state.withLock { state in
            guard state.cancelledIds.remove(sleeper.id) == nil else { return (true, []) }

            state.sleepers.append(sleeper)

            let pendingCount = state.pendingCount(of: sleeper.duration)
            let isReady: (Waiter) -> Bool = { $0.duration == sleeper.duration && $0.count <= pendingCount }
            let ready = state.waiters.filter(isReady)
            state.waiters.removeAll(where: isReady)

            return (false, ready)
        }

        if isCancelled {
            sleeper.continuation.resume(throwing: CancellationError())
        }

        readyWaiters.forEach { $0.continuation.resume() }
    }

    /// Cancellation can arrive before registration, so an unknown id is remembered for `register`.
    func cancel(id: UUID) {
        let sleeper: Sleeper? = state.withLock { state in
            guard let index = state.sleepers.firstIndex(where: { $0.id == id }) else {
                state.cancelledIds.insert(id)
                return nil
            }

            return state.sleepers.remove(at: index)
        }

        sleeper?.continuation.resume(throwing: CancellationError())
    }
}

private extension ManualClock.State {
    func pendingCount(of duration: Duration) -> Int {
        sleepers.filter { $0.duration == duration }.count
    }
}
