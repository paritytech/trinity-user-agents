import Foundation

/// Thrown by ``withTimeout(_:operation:)`` when the operation does not complete
/// before the timeout elapses.
public struct TimeoutError: Error, Sendable {
    public init() {}
}

/// Runs `operation` and throws ``TimeoutError`` if it does not finish within `duration`.
public func withTimeout<T: Sendable>(
    _ duration: Duration,
    operation: @Sendable @escaping () async throws -> T
) async throws -> T {
    try await withTimeout(duration, clock: ContinuousClock(), operation: operation)
}

/// Like ``withTimeout(_:operation:)`` but paces the deadline on `clock`, so a caller that injects a
/// test clock can drive the timeout without waiting on the wall clock.
public func withTimeout<T: Sendable>(
    _ duration: Duration,
    clock: any Clock<Duration>,
    operation: @Sendable @escaping () async throws -> T
) async throws -> T {
    try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask(operation: operation)
        group.addTask {
            try await clock.sleep(for: duration)
            throw TimeoutError()
        }

        defer { group.cancelAll() }

        guard let result = try await group.next() else {
            throw TimeoutError()
        }
        return result
    }
}
