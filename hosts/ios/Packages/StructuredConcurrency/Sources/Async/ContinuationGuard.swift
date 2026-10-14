import os

/// Thread-safe one-shot gate ensuring a checked throwing continuation is resumed exactly once.
///
/// Useful when both a success callback and a timeout task race to resume the same continuation.
/// Whichever side calls `resume` first wins; subsequent calls are silently ignored.
public final class CheckedContinuationGuard<T>: Sendable {
    private let state: OSAllocatedUnfairLock<Bool>
    private let continuation: CheckedContinuation<T, any Error>

    public init(_ continuation: CheckedContinuation<T, any Error>) {
        state = OSAllocatedUnfairLock(initialState: false)
        self.continuation = continuation
    }

    public func resume(_ result: Result<T, Error>) {
        let shouldResume = state.withLock { wasResumed in
            guard !wasResumed else { return false }

            wasResumed = true

            return true
        }

        if shouldResume {
            continuation.resume(with: result)
        }
    }

    public func resume(returning value: T) {
        resume(.success(value))
    }

    public func resume(throwing error: any Error) {
        resume(.failure(error))
    }
}
