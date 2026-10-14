import os

/// State for tracking a continuation and handling cancellation races.
private enum PendingState<Value: Sendable> {
    case none
    case cancelledBeforeReg
    case continuation(CheckedContinuation<Value, Error>)
}

/// Coalesces concurrent async calls into a single in-flight task.
///
/// When multiple callers invoke ``run(_:)`` concurrently, only the first
/// starts the work. Subsequent callers await the same `Task` and receive
/// the same result. Once the task completes, the next call starts fresh.
///
/// Cancellation semantics: if a joiner's surrounding task is cancelled,
/// the joiner immediately throws `CancellationError` and abandons its wait.
/// The shared work continues uninterrupted, and other joiners still receive
/// its result. The shared task is never cancelled.
///
/// ```swift
/// let fetch = CoalescingTask<String>()
/// let token = try await fetch.run { try await obtainToken() }
/// ```
public final class CoalescingTask<Value: Sendable>: Sendable {
    private let inflight = OSAllocatedUnfairLock<Task<Value, Error>?>(initialState: nil)

    public init() {}

    public func run(_ operation: @Sendable @escaping () async throws -> Value) async throws -> Value {
        let task: Task<Value, Error> = inflight.withLock { current in
            if let existing = current {
                return existing
            }
            let newTask = Task<Value, Error> {
                defer { self.inflight.withLock { $0 = nil } }
                return try await operation()
            }
            current = newTask
            return newTask
        }

        let state = OSAllocatedUnfairLock<PendingState<Value>>(initialState: .none)

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let shouldCancelNow = state.withLock { current -> Bool in
                    switch current {
                    case .cancelledBeforeReg:
                        return true
                    case .none:
                        current = .continuation(continuation)
                        return false
                    case .continuation:
                        // Shouldn't happen but be defensive
                        return false
                    }
                }

                if shouldCancelNow {
                    continuation.resume(throwing: CancellationError())
                } else {
                    Task {
                        let result = await task.result
                        let pending = state.withLock { current -> CheckedContinuation<Value, Error>? in
                            guard case let .continuation(cont) = current else { return nil }
                            current = .none
                            return cont
                        }
                        if let cont = pending {
                            cont.resume(with: result)
                        }
                    }
                }
            }
        } onCancel: {
            let pending = state.withLock { current -> CheckedContinuation<Value, Error>? in
                switch current {
                case let .continuation(cont):
                    current = .none
                    return cont
                case .none:
                    // Not yet registered, mark as cancelled
                    current = .cancelledBeforeReg
                    return nil
                case .cancelledBeforeReg:
                    // Already marked, shouldn't happen
                    return nil
                }
            }
            if let cont = pending {
                cont.resume(throwing: CancellationError())
            }
        }
    }
}
