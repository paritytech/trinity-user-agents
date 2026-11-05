import Foundation

public protocol RetryStrategy: Sendable {
    mutating func decideOnFailure(_ error: Error) async throws
}

public func withRetry<T: Sendable>(
    strategy: some RetryStrategy,
    operation: @Sendable () async throws -> T
) async throws -> T {
    var strategy = strategy

    while true {
        do {
            return try await operation()
        } catch {
            try Task.checkCancellation()
            try await strategy.decideOnFailure(error)
        }
    }
}
