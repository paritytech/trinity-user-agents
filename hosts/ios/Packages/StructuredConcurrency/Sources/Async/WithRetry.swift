import Foundation

public enum RetryError: Error {
    case limitReached
}

/// Runs `operation`, retrying up to `maxAttempts` times with exponential backoff on failure.
///
/// The delay between retries doubles after each attempt, starting from `initialDelay`.
/// If the task is cancelled at any point, the function rethrows `CancellationError`
/// without further retries.
///
/// - Parameters:
///   - maxAttempts: Total number of attempts (including the first). Must be at least 1.
///   - initialDelay: Delay before the first retry. Doubles after each subsequent retry.
///   - shouldRetry: Returns false for errors that must fail at once without further attempts.
///   - operation: The async throwing closure to execute.
/// - Returns: The result of a successful `operation` invocation.
public func withRetry<T: Sendable>(
    maxAttempts: Int,
    initialDelay: Duration = .seconds(1),
    shouldRetry: @Sendable @escaping (Error) -> Bool = { _ in true },
    operation: @Sendable @escaping () async throws -> T
) async throws -> T {
    guard maxAttempts > 0 else {
        throw RetryError.limitReached
    }

    var delay = initialDelay

    for attempt in 1 ... maxAttempts {
        do {
            return try await operation()
        } catch {
            try Task.checkCancellation()

            guard shouldRetry(error) else {
                throw error
            }

            let isLastAttempt = attempt == maxAttempts

            if isLastAttempt {
                throw error
            }

            try await Task.sleep(for: delay)
            delay *= 2
        }
    }

    throw RetryError.limitReached
}
