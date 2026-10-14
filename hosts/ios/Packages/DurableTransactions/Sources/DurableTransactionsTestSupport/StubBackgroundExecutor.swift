import BackgroundExecution
import Foundation

/// Runs the operation directly; there is no OS background window to assert in a test.
public struct StubBackgroundExecutor: BackgroundExecuting {
    public init() {}

    public func execute<T: Sendable>(_ operation: @Sendable () async throws -> T) async throws -> T {
        try await operation()
    }
}
